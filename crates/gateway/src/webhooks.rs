//! Live webhooks service implementation wiring the webhooks crate into gateway services.

use std::sync::Arc;

use base64::Engine as _;
use {async_trait::async_trait, serde_json::Value, tracing::error};

use moltis_webhooks::{
    profiles::ProfileRegistry,
    store::WebhookStore,
    types::{WebhookCreate, WebhookPatch},
};

use crate::services::{ServiceError, ServiceResult, WebhooksService as WebhooksServiceTrait};

/// Gateway-facing webhooks service backed by the real store.
pub struct LiveWebhooksService {
    store: Arc<dyn WebhookStore>,
    profiles: ProfileRegistry,
}

impl LiveWebhooksService {
    pub fn new(store: Arc<dyn WebhookStore>) -> Self {
        Self {
            store,
            profiles: ProfileRegistry::new(),
        }
    }

    pub fn store(&self) -> &Arc<dyn WebhookStore> {
        &self.store
    }
}

#[async_trait]
impl WebhooksServiceTrait for LiveWebhooksService {
    async fn list(&self) -> ServiceResult {
        let webhooks = self.store.list_webhooks().await.map_err(|e| {
            error!(error = %e, "webhooks list failed");
            ServiceError::message(e)
        })?;
        Ok(serde_json::to_value(webhooks)?)
    }

    async fn get(&self, params: Value) -> ServiceResult {
        let id = params
            .get("id")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| ServiceError::message("missing 'id'"))?;
        let webhook = self.store.get_webhook(id).await.map_err(|e| {
            error!(error = %e, "webhooks get failed");
            ServiceError::message(e)
        })?;
        Ok(serde_json::to_value(webhook)?)
    }

    async fn create(&self, params: Value) -> ServiceResult {
        let create: WebhookCreate = serde_json::from_value(params)
            .map_err(|e| ServiceError::message(format!("invalid webhook spec: {e}")))?;
        let webhook = self.store.create_webhook(create).await.map_err(|e| {
            error!(error = %e, "webhooks create failed");
            ServiceError::message(e)
        })?;
        Ok(serde_json::to_value(webhook)?)
    }

    async fn update(&self, params: Value) -> ServiceResult {
        let id = params
            .get("id")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| ServiceError::message("missing 'id'"))?;
        let patch: WebhookPatch = serde_json::from_value(
            params
                .get("patch")
                .cloned()
                .unwrap_or(Value::Object(Default::default())),
        )
        .map_err(|e| ServiceError::message(format!("invalid patch: {e}")))?;
        let webhook = self.store.update_webhook(id, patch).await.map_err(|e| {
            error!(error = %e, "webhooks update failed");
            ServiceError::message(e)
        })?;
        Ok(serde_json::to_value(webhook)?)
    }

    async fn delete(&self, params: Value) -> ServiceResult {
        let id = params
            .get("id")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| ServiceError::message("missing 'id'"))?;
        self.store.delete_webhook(id).await.map_err(|e| {
            error!(error = %e, "webhooks delete failed");
            ServiceError::message(e)
        })?;
        Ok(serde_json::json!({ "deleted": id }))
    }

    async fn deliveries(&self, params: Value) -> ServiceResult {
        let webhook_id = params
            .get("webhookId")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| ServiceError::message("missing 'webhookId'"))?;
        let limit = params
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(50) as usize;
        let offset = params
            .get("offset")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
        let deliveries = self
            .store
            .list_deliveries(webhook_id, limit, offset)
            .await
            .map_err(|e| {
                error!(error = %e, "webhooks deliveries list failed");
                ServiceError::message(e)
            })?;
        Ok(serde_json::to_value(deliveries)?)
    }

    async fn delivery_get(&self, params: Value) -> ServiceResult {
        let id = params
            .get("id")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| ServiceError::message("missing 'id'"))?;
        let delivery = self.store.get_delivery(id).await.map_err(|e| {
            error!(error = %e, "webhooks delivery get failed");
            ServiceError::message(e)
        })?;
        Ok(serde_json::to_value(delivery)?)
    }

    async fn delivery_payload(&self, params: Value) -> ServiceResult {
        let id = params
            .get("id")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| ServiceError::message("missing 'id'"))?;
        let body = self.store.get_delivery_body(id).await.map_err(|e| {
            error!(error = %e, "webhooks delivery payload failed");
            ServiceError::message(e)
        })?;
        match body {
            Some(bytes) => {
                // Try to parse as JSON for pretty display
                if let Ok(json) = serde_json::from_slice::<Value>(&bytes) {
                    Ok(json)
                } else {
                    Ok(serde_json::json!({
                        "raw": base64::engine::general_purpose::STANDARD.encode(&bytes),
                        "size": bytes.len(),
                    }))
                }
            }
            None => Ok(serde_json::json!(null)),
        }
    }

    async fn delivery_actions(&self, params: Value) -> ServiceResult {
        let delivery_id = params
            .get("deliveryId")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| ServiceError::message("missing 'deliveryId'"))?;
        let actions = self
            .store
            .list_response_actions(delivery_id)
            .await
            .map_err(|e| {
                error!(error = %e, "webhooks delivery actions failed");
                ServiceError::message(e)
            })?;
        Ok(serde_json::to_value(actions)?)
    }

    async fn profiles(&self) -> ServiceResult {
        let profiles = self.profiles.list();
        Ok(serde_json::to_value(profiles)?)
    }
}
