//! Core hook types shared across crates.
//!
//! These types define the hook event system. The full registry and shell handler
//! live in `moltis-plugins`; this module provides the trait and types needed by
//! crates like `moltis-agents` that cannot depend on plugins.

use std::{collections::HashMap, fmt, sync::Arc};

use {
    anyhow::Result,
    async_trait::async_trait,
    serde::{Deserialize, Serialize},
    serde_json::Value,
    tracing::{debug, info, warn},
};

// ── HookEvent ───────────────────────────────────────────────────────────────

/// Lifecycle events that hooks can subscribe to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HookEvent {
    BeforeAgentStart,
    AgentEnd,
    BeforeCompaction,
    AfterCompaction,
    MessageReceived,
    MessageSending,
    MessageSent,
    BeforeToolCall,
    AfterToolCall,
    ToolResultPersist,
    SessionStart,
    SessionEnd,
    GatewayStart,
    GatewayStop,
}

impl fmt::Display for HookEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl HookEvent {
    /// All variants, for iteration.
    pub const ALL: &'static [HookEvent] = &[
        Self::BeforeAgentStart,
        Self::AgentEnd,
        Self::BeforeCompaction,
        Self::AfterCompaction,
        Self::MessageReceived,
        Self::MessageSending,
        Self::MessageSent,
        Self::BeforeToolCall,
        Self::AfterToolCall,
        Self::ToolResultPersist,
        Self::SessionStart,
        Self::SessionEnd,
        Self::GatewayStart,
        Self::GatewayStop,
    ];
}

// ── HookPayload ─────────────────────────────────────────────────────────────

/// Typed payload carried with each hook event.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event")]
pub enum HookPayload {
    BeforeAgentStart {
        session_key: String,
        model: String,
    },
    AgentEnd {
        session_key: String,
        text: String,
        iterations: usize,
        tool_calls: usize,
    },
    BeforeCompaction {
        session_key: String,
        message_count: usize,
    },
    AfterCompaction {
        session_key: String,
        summary_len: usize,
    },
    MessageReceived {
        session_key: String,
        content: String,
        channel: Option<String>,
    },
    MessageSending {
        session_key: String,
        content: String,
    },
    MessageSent {
        session_key: String,
        content: String,
    },
    BeforeToolCall {
        session_key: String,
        tool_name: String,
        arguments: Value,
    },
    AfterToolCall {
        session_key: String,
        tool_name: String,
        success: bool,
        result: Option<Value>,
    },
    ToolResultPersist {
        session_key: String,
        tool_name: String,
        result: Value,
    },
    SessionStart {
        session_key: String,
    },
    SessionEnd {
        session_key: String,
    },
    GatewayStart {
        address: String,
    },
    GatewayStop,
}

impl HookPayload {
    /// Returns the [`HookEvent`] variant that matches this payload.
    pub fn event(&self) -> HookEvent {
        match self {
            Self::BeforeAgentStart { .. } => HookEvent::BeforeAgentStart,
            Self::AgentEnd { .. } => HookEvent::AgentEnd,
            Self::BeforeCompaction { .. } => HookEvent::BeforeCompaction,
            Self::AfterCompaction { .. } => HookEvent::AfterCompaction,
            Self::MessageReceived { .. } => HookEvent::MessageReceived,
            Self::MessageSending { .. } => HookEvent::MessageSending,
            Self::MessageSent { .. } => HookEvent::MessageSent,
            Self::BeforeToolCall { .. } => HookEvent::BeforeToolCall,
            Self::AfterToolCall { .. } => HookEvent::AfterToolCall,
            Self::ToolResultPersist { .. } => HookEvent::ToolResultPersist,
            Self::SessionStart { .. } => HookEvent::SessionStart,
            Self::SessionEnd { .. } => HookEvent::SessionEnd,
            Self::GatewayStart { .. } => HookEvent::GatewayStart,
            Self::GatewayStop => HookEvent::GatewayStop,
        }
    }
}

// ── HookAction ──────────────────────────────────────────────────────────────

/// The outcome a hook handler returns.
#[derive(Debug)]
pub enum HookAction {
    /// Let the event proceed normally.
    Continue,
    /// Replace part of the payload data (e.g. modify tool arguments or results).
    ModifyPayload(Value),
    /// Block the action entirely, with a reason string.
    Block(String),
}

impl Default for HookAction {
    fn default() -> Self {
        Self::Continue
    }
}

// ── HookHandler trait ───────────────────────────────────────────────────────

/// Trait implemented by both native and shell hook handlers.
#[async_trait]
pub trait HookHandler: Send + Sync {
    /// A human-readable name for this handler.
    fn name(&self) -> &str;

    /// Which events this handler subscribes to.
    fn events(&self) -> &[HookEvent];

    /// Handle the event, returning an action that may modify or block the flow.
    async fn handle(&self, event: HookEvent, payload: &HookPayload) -> Result<HookAction>;
}

// ── HookRegistry ────────────────────────────────────────────────────────────

/// Manages registered hook handlers and dispatches events to them.
pub struct HookRegistry {
    handlers: HashMap<HookEvent, Vec<Arc<dyn HookHandler>>>,
}

impl HookRegistry {
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
        }
    }

    /// Register a handler for all events it subscribes to.
    pub fn register(&mut self, handler: Arc<dyn HookHandler>) {
        for &event in handler.events() {
            self.handlers
                .entry(event)
                .or_default()
                .push(Arc::clone(&handler));
        }
        info!(handler = handler.name(), "hook handler registered");
    }

    /// Returns true if any handlers are registered for the given event.
    pub fn has_handlers(&self, event: HookEvent) -> bool {
        self.handlers.get(&event).is_some_and(|v| !v.is_empty())
    }

    /// Dispatch an event to all registered handlers in order.
    ///
    /// - Returns the first [`HookAction::Block`] encountered (short-circuits).
    /// - Returns the last [`HookAction::ModifyPayload`] if any.
    /// - Otherwise returns [`HookAction::Continue`].
    pub async fn dispatch(&self, payload: &HookPayload) -> Result<HookAction> {
        let event = payload.event();
        let handlers = match self.handlers.get(&event) {
            Some(h) if !h.is_empty() => h,
            _ => return Ok(HookAction::Continue),
        };

        debug!(event = %event, count = handlers.len(), "dispatching hook event");

        let mut last_modify: Option<Value> = None;

        for handler in handlers {
            match handler.handle(event, payload).await {
                Ok(HookAction::Continue) => {},
                Ok(HookAction::ModifyPayload(v)) => {
                    debug!(handler = handler.name(), event = %event, "hook modified payload");
                    last_modify = Some(v);
                },
                Ok(HookAction::Block(reason)) => {
                    info!(handler = handler.name(), event = %event, reason = %reason, "hook blocked event");
                    return Ok(HookAction::Block(reason));
                },
                Err(e) => {
                    warn!(handler = handler.name(), event = %event, error = %e, "hook handler failed");
                },
            }
        }

        Ok(match last_modify {
            Some(v) => HookAction::ModifyPayload(v),
            None => HookAction::Continue,
        })
    }
}

impl Default for HookRegistry {
    fn default() -> Self {
        Self::new()
    }
}
