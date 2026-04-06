//! Background worker for processing webhook deliveries.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{error, info, instrument, warn};

use crate::{
    profiles::SourceProfile,
    store::{DeliveryUpdate, WebhookStore},
    types::DeliveryStatus,
};

/// Result of processing a webhook delivery via chat.send_sync.
pub struct ProcessResult {
    pub output: Option<String>,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub session_key: String,
}

/// Callback type for executing an agent turn.
pub type ExecuteFn = Arc<
    dyn Fn(
            ExecuteRequest,
        )
            -> Pin<Box<dyn Future<Output = anyhow::Result<ProcessResult>> + Send>>
        + Send
        + Sync,
>;

/// Request to execute a webhook delivery.
pub struct ExecuteRequest {
    pub webhook_id: i64,
    pub delivery_id: i64,
    pub session_key: String,
    pub agent_id: Option<String>,
    pub model: Option<String>,
    pub message: String,
}

/// Background worker that processes queued webhook deliveries.
pub struct WebhookWorker {
    rx: mpsc::Receiver<i64>,
    store: Arc<dyn WebhookStore>,
    execute_fn: ExecuteFn,
}

impl WebhookWorker {
    pub fn new(
        rx: mpsc::Receiver<i64>,
        store: Arc<dyn WebhookStore>,
        execute_fn: ExecuteFn,
    ) -> Self {
        Self {
            rx,
            store,
            execute_fn,
        }
    }

    /// Run the worker loop, processing deliveries from the channel.
    #[instrument(skip_all, name = "webhook_worker")]
    pub async fn run(mut self) {
        info!("webhook worker started");
        while let Some(delivery_id) = self.rx.recv().await {
            if let Err(e) = self.process_delivery(delivery_id).await {
                error!(delivery_id, error = %e, "webhook delivery processing failed");
            }
        }
        info!("webhook worker stopped");
    }

    #[instrument(skip(self), fields(delivery_id))]
    async fn process_delivery(&self, delivery_id: i64) -> anyhow::Result<()> {
        // Mark as processing
        self.store
            .update_delivery_status(
                delivery_id,
                DeliveryStatus::Processing,
                DeliveryUpdate {
                    started_at: Some(now_iso()),
                    ..Default::default()
                },
            )
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        // Load delivery and webhook
        let delivery = self
            .store
            .get_delivery(delivery_id)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let webhook = self
            .store
            .get_webhook(delivery.webhook_id)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        // Load body for normalization
        let body_bytes = self
            .store
            .get_delivery_body(delivery_id)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?
            .unwrap_or_default();

        let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap_or_default();

        // Get the source profile for normalization
        let profile_registry = crate::profiles::ProfileRegistry::new();
        let profile = profile_registry.get(&webhook.source_profile);

        // Normalize payload
        let event_type = delivery.event_type.as_deref().unwrap_or("unknown");
        let normalized = match profile {
            Some(p) => p.normalize_payload(event_type, &body),
            None => {
                crate::profiles::generic::GenericProfile.normalize_payload(event_type, &body)
            }
        };

        // Build the delivery message
        let message = crate::normalize::build_delivery_message(
            &webhook,
            delivery.event_type.as_deref(),
            delivery.delivery_key.as_deref(),
            &delivery.received_at,
            &normalized.summary,
        );

        // Build session key based on session mode
        let session_key = match webhook.session_mode {
            crate::types::SessionMode::PerDelivery => {
                format!("webhook:{}:{}", webhook.public_id, delivery_id)
            }
            crate::types::SessionMode::PerEntity => match &delivery.entity_key {
                Some(ek) => format!("webhook:{ek}"),
                None => format!("webhook:{}:{}", webhook.public_id, delivery_id),
            },
            crate::types::SessionMode::NamedSession => webhook
                .named_session_key
                .clone()
                .unwrap_or_else(|| format!("webhook:{}", webhook.public_id)),
        };

        let start = std::time::Instant::now();

        // Execute via callback
        let result = (self.execute_fn)(ExecuteRequest {
            webhook_id: webhook.id,
            delivery_id,
            session_key: session_key.clone(),
            agent_id: webhook.agent_id.clone(),
            model: webhook.model.clone(),
            message,
        })
        .await;

        let duration_ms = start.elapsed().as_millis() as i64;

        match result {
            Ok(process_result) => {
                self.store
                    .update_delivery_status(
                        delivery_id,
                        DeliveryStatus::Completed,
                        DeliveryUpdate {
                            session_key: Some(process_result.session_key),
                            finished_at: Some(now_iso()),
                            duration_ms: Some(duration_ms),
                            input_tokens: process_result.input_tokens,
                            output_tokens: process_result.output_tokens,
                            ..Default::default()
                        },
                    )
                    .await
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
            }
            Err(e) => {
                warn!(delivery_id, error = %e, "webhook execution failed");
                self.store
                    .update_delivery_status(
                        delivery_id,
                        DeliveryStatus::Failed,
                        DeliveryUpdate {
                            run_error: Some(e.to_string()),
                            finished_at: Some(now_iso()),
                            duration_ms: Some(duration_ms),
                            ..Default::default()
                        },
                    )
                    .await
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
            }
        }

        Ok(())
    }
}

fn now_iso() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".into())
}
