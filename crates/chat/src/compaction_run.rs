//! Compaction strategy dispatcher.
//!
//! Routes a session history through the [`CompactionMode`] selected in
//! `chat.compaction`. Each strategy builds and returns the replacement
//! history; call sites handle storage and broadcast.
//!
//! See `docs/src/compaction.md` for the full mode comparison and trade-off
//! guidance, and the module rustdoc on [`moltis_config::CompactionMode`] for
//! per-variant semantics.

use {
    moltis_agents::model::LlmProvider,
    moltis_config::{CompactionConfig, CompactionMode},
    moltis_sessions::{MessageContent, PersistedMessage},
    serde_json::Value,
    thiserror::Error,
    tracing::info,
};
#[cfg(feature = "llm-compaction")]
use {
    moltis_agents::model::{ChatMessage, StreamEvent, values_to_chat_messages},
    tokio_stream::StreamExt,
};

/// Errors surfaced by [`run_compaction`].
///
/// Several variants are gated on the `llm-compaction` cargo feature; when
/// the feature is off the LLM-backed strategies aren't compiled in, so
/// their dedicated error variants become dead code.
#[derive(Debug, Error)]
pub(crate) enum CompactionRunError {
    /// History was empty — nothing to compact.
    #[error("nothing to compact")]
    EmptyHistory,
    /// The strategy produced no summary text.
    #[error("compact produced empty summary")]
    EmptySummary,
    /// A mode that requires an LLM provider was selected but none was passed.
    #[cfg(feature = "llm-compaction")]
    #[error("compaction mode '{mode}' requires an LLM provider to be available for the session")]
    ProviderRequired { mode: &'static str },
    /// `recency_preserving` couldn't make meaningful progress: the history is
    /// already smaller than `protect_head + protect_tail_min + 1`, and there
    /// was no bulky tool-result content to prune. The caller should fall
    /// back to a different mode (e.g. `deterministic`) rather than loop.
    #[error(
        "history has {messages} messages — too small for recency_preserving with \
         protect_head={head} and protect_tail_min={tail}; no tool-result pruning \
         was possible either. Try chat.compaction.mode = \"deterministic\" for \
         tiny sessions."
    )]
    TooSmallToCompact {
        messages: usize,
        head: usize,
        tail: usize,
    },
    /// The user selected a mode that requires a cargo feature that isn't enabled.
    #[cfg(not(feature = "llm-compaction"))]
    #[error("compaction mode '{mode}' requires the 'llm-compaction' cargo feature to be enabled")]
    FeatureDisabled { mode: &'static str },
    /// The LLM streaming summary call failed.
    #[cfg(feature = "llm-compaction")]
    #[error("compact summarization failed: {0}")]
    LlmFailed(String),
}

/// Best-effort extraction of a human-readable summary body from a
/// compacted history, for use in memory-file snapshots and hook payloads.
///
/// Walks the compacted messages looking for the first one whose text
/// content begins with either `[Conversation Summary]` (produced by the
/// `deterministic`, `structured`, and `llm_replace` modes) or
/// `[Conversation Compacted]` (produced by the `recency_preserving`
/// middle marker), and returns the stripped body. Returns an empty string
/// when no summary-shaped message is found, which is fine for hook
/// `summary_len` reporting and falls through gracefully.
#[must_use]
pub(crate) fn extract_summary_body(compacted: &[Value]) -> String {
    compacted
        .iter()
        .filter_map(|msg| msg.get("content").and_then(Value::as_str))
        .find_map(|content| {
            content
                .strip_prefix("[Conversation Summary]\n\n")
                .or_else(|| content.strip_prefix("[Conversation Compacted]\n\n"))
                .map(str::to_string)
        })
        .unwrap_or_default()
}

/// Run the compaction strategy selected by `config` against `history`.
///
/// Returns the replacement history vec. Call sites are responsible for
/// writing the result back to the session store.
///
/// `provider` is only consulted by LLM-backed modes; pass `None` when no
/// provider has been resolved for the session. LLM modes return
/// [`CompactionRunError::ProviderRequired`] when called without one.
pub(crate) async fn run_compaction(
    history: &[Value],
    config: &CompactionConfig,
    provider: Option<&dyn LlmProvider>,
) -> Result<Vec<Value>, CompactionRunError> {
    if history.is_empty() {
        return Err(CompactionRunError::EmptyHistory);
    }

    match config.mode {
        CompactionMode::Deterministic => deterministic_strategy(history),
        CompactionMode::LlmReplace => {
            #[cfg(feature = "llm-compaction")]
            {
                let provider = provider.ok_or(CompactionRunError::ProviderRequired {
                    mode: "llm_replace",
                })?;
                llm_replace_strategy(history, config, provider).await
            }
            #[cfg(not(feature = "llm-compaction"))]
            {
                let _ = (config, provider);
                Err(CompactionRunError::FeatureDisabled {
                    mode: "llm_replace",
                })
            }
        },
        CompactionMode::RecencyPreserving => {
            let context_window = provider.map_or(200_000, LlmProvider::context_window);
            recency_preserving_strategy(history, config, context_window)
        },
        CompactionMode::Structured => {
            #[cfg(feature = "llm-compaction")]
            {
                let provider =
                    provider.ok_or(CompactionRunError::ProviderRequired { mode: "structured" })?;
                let context_window = provider.context_window();
                structured_strategy(history, config, context_window, provider).await
            }
            #[cfg(not(feature = "llm-compaction"))]
            {
                let _ = (config, provider);
                Err(CompactionRunError::FeatureDisabled { mode: "structured" })
            }
        },
    }
}

/// `CompactionMode::Deterministic` strategy — current PR #653 behaviour.
///
/// Runs the structured-extraction helpers in `crate::compaction`, compresses
/// the summary to fit the budget, and wraps it in a single user message.
fn deterministic_strategy(history: &[Value]) -> Result<Vec<Value>, CompactionRunError> {
    let merged = crate::compaction::compute_compaction_summary(history)
        .ok_or(CompactionRunError::EmptySummary)?;
    let summary = crate::compaction::compress_summary(&merged);
    if summary.is_empty() {
        return Err(CompactionRunError::EmptySummary);
    }

    info!(
        messages = history.len(),
        "chat.compact: deterministic summary"
    );

    Ok(vec![build_summary_message(
        &crate::compaction::get_compact_continuation_message(&summary, false),
    )])
}

/// Head/tail boundaries computed by `compute_boundaries`.
///
/// `head_end` is the exclusive index marking the end of the verbatim head
/// region; `tail_start` is the inclusive index marking the beginning of the
/// verbatim tail. The middle slice is `history[head_end..tail_start]` and
/// may be empty when the session is already small enough to fit in the
/// retained budget.
struct HeadTailBounds {
    head_end: usize,
    tail_start: usize,
    protect_head: usize,
    protect_tail_min: usize,
}

/// Compute the head / middle / tail boundaries for a recency-aware strategy.
///
/// Never splits a tool_use / tool_result group — the tail boundary slides
/// forward past any consecutive tool-result run so the kept slice always
/// starts on a non-tool message (or the end of the history).
fn compute_boundaries(
    history: &[Value],
    config: &CompactionConfig,
    context_window: u32,
) -> HeadTailBounds {
    let n = history.len();
    let protect_head = (config.protect_head as usize).min(n);
    let protect_tail_min = (config.protect_tail_min as usize).min(n.saturating_sub(protect_head));

    // Convert fractional budgets to a concrete token count. Clamp everything
    // into a sane range so wildly misconfigured ratios don't divide by zero.
    let threshold = config.threshold_percent.clamp(0.1, 0.95);
    let tail_ratio = config.tail_budget_ratio.clamp(0.05, 0.80);
    let tail_budget_tokens =
        ((f64::from(context_window) * f64::from(threshold) * f64::from(tail_ratio)).round() as u64)
            .max(1);

    // Walk backward from the end until either the budget is consumed or the
    // floor (protect_tail_min) is satisfied. Whichever covers more messages
    // wins, so small sessions still keep the floor even when their tail is
    // tiny and large sessions honour the token budget when the floor is too
    // small.
    let head_end = protect_head;
    let mut accumulated: u64 = 0;
    let mut tail_start = n;
    for idx in (head_end..n).rev() {
        let msg_tokens = message_tokens(&history[idx]);
        let keep_for_budget = accumulated + msg_tokens <= tail_budget_tokens;
        let keep_for_floor = (n - idx) <= protect_tail_min;
        if keep_for_budget || keep_for_floor {
            accumulated += msg_tokens;
            tail_start = idx;
        } else {
            break;
        }
    }

    // Never split a tool_use / tool_result group: if the boundary falls on a
    // tool-result message, walk forward past the whole group (the parent
    // assistant message and any siblings).
    let tail_start = align_boundary_forward_past_tool_group(history, tail_start);

    HeadTailBounds {
        head_end,
        tail_start,
        protect_head,
        protect_tail_min,
    }
}

/// Prune bulky tool-result content in anything older than the last
/// `protect_tail_min * 3` messages, then repair orphaned tool_call /
/// tool_result pairs so strict providers accept the retry.
fn finalize_kept(
    mut kept: Vec<Value>,
    config: &CompactionConfig,
    protect_tail_min: usize,
) -> Result<Vec<Value>, CompactionRunError> {
    let tool_prune_frontier = kept.len().saturating_sub(protect_tail_min * 3);
    prune_tool_results_before(
        &mut kept,
        tool_prune_frontier,
        config.tool_prune_char_threshold,
    );
    sanitize_tool_pairs(kept)
}

/// Handle the "head and tail already cover everything" fallback.
///
/// Prune bulky tool-result content in place; if nothing actually changed,
/// return `TooSmallToCompact` so the caller can fall back to a different
/// mode instead of retrying forever.
fn in_place_prune_or_err(
    history: &[Value],
    config: &CompactionConfig,
    bounds: &HeadTailBounds,
) -> Result<Vec<Value>, CompactionRunError> {
    let mut kept: Vec<Value> = history.to_vec();
    let kept_len = kept.len();
    let pruned = prune_tool_results_before(&mut kept, kept_len, config.tool_prune_char_threshold);
    if pruned == 0 {
        return Err(CompactionRunError::TooSmallToCompact {
            messages: history.len(),
            head: bounds.protect_head,
            tail: bounds.protect_tail_min,
        });
    }
    sanitize_tool_pairs(kept)
}

/// `CompactionMode::RecencyPreserving` strategy — head + middle-prune + tail.
///
/// Keeps the first `config.protect_head` messages and a token-budget tail
/// verbatim. The middle is collapsed into a single marker message; any bulky
/// tool-result content that survives in the head or tail is replaced with a
/// placeholder so the retry fits inside the model's context window. After the
/// splice, orphaned tool_use / tool_result pairs are repaired so strict
/// providers don't reject the retry.
///
/// No LLM calls. Inspired by `hermes-agent`'s `ContextCompressor` tool-output
/// pruning phase and openclaw's `repairToolUseResultPairing`.
fn recency_preserving_strategy(
    history: &[Value],
    config: &CompactionConfig,
    context_window: u32,
) -> Result<Vec<Value>, CompactionRunError> {
    let bounds = compute_boundaries(history, config, context_window);
    let HeadTailBounds {
        head_end,
        tail_start,
        protect_tail_min,
        ..
    } = bounds;
    let n = history.len();

    if head_end >= tail_start {
        return in_place_prune_or_err(history, config, &bounds);
    }

    let mut kept: Vec<Value> = Vec::with_capacity(head_end + 1 + (n - tail_start));
    kept.extend(history[..head_end].iter().cloned());

    let middle = &history[head_end..tail_start];
    if !middle.is_empty() {
        kept.push(build_middle_marker(middle));
    }

    kept.extend(history[tail_start..].iter().cloned());

    let kept = finalize_kept(kept, config, protect_tail_min)?;

    info!(
        input_messages = n,
        output_messages = kept.len(),
        head = head_end,
        middle = tail_start - head_end,
        tail = n - tail_start,
        "chat.compact: recency_preserving"
    );

    Ok(kept)
}

/// Structured-summary template used by `CompactionMode::Structured`.
///
/// Mirrors the convention used by `hermes-agent`'s `ContextCompressor` and
/// `openclaw`'s `safeguard` compaction — Goal / Progress / Decisions /
/// Files / Next Steps. Kept verbatim here so future edits are easy to
/// diff and so test fixtures can match against the literal template.
#[cfg(feature = "llm-compaction")]
const STRUCTURED_TEMPLATE: &str = "\
## Goal
[What the user is trying to accomplish]

## Constraints & Preferences
[User preferences, coding style, constraints, important decisions]

## Progress
### Done
[Completed work — include specific file paths, commands run, results obtained]
### In Progress
[Work currently underway]
### Blocked
[Any blockers or issues encountered]

## Key Decisions
[Important technical decisions and why they were made]

## Relevant Files
[Files read, modified, or created — with brief note on each]

## Next Steps
[What needs to happen next to continue the work]

## Critical Context
[Any specific values, error messages, configuration details, or data that would be lost without explicit preservation]";

/// System-message instructions that frame the structured summary call.
#[cfg(feature = "llm-compaction")]
const STRUCTURED_SYSTEM_INSTRUCTIONS: &str = "\
You are a conversation summarizer. The messages that follow are an agentic \
coding session you must summarize. Your summary must capture: active tasks \
and their current status (in-progress, blocked, pending); batch operation \
progress; the last thing the user asked for and what was being done about \
it; decisions made and their rationale; TODOs, open questions, and \
constraints; any commitments or follow-ups promised. Prioritize recent \
context over older history. Preserve all opaque identifiers exactly as \
written (no shortening or reconstruction): UUIDs, hashes, tokens, API \
keys, hostnames, IPs, ports, URLs, and file names. After the conversation, \
you will receive a final instruction telling you which template to fill in.";

/// User-message instructions for the first compaction of a session.
#[cfg(feature = "llm-compaction")]
fn structured_first_compaction_instructions() -> String {
    format!(
        "Produce a structured handoff summary for a later assistant that will \
         continue this conversation after the earlier turns above are compacted. \
         Use this exact structure, filling every section (write \"(none)\" if a \
         section has nothing to report):\n\n{STRUCTURED_TEMPLATE}\n\n\
         Target roughly 800 tokens. Be specific — include file paths, command \
         outputs, error messages, and concrete values rather than vague \
         descriptions. Write only the summary body. Do not include any preamble \
         or prefix."
    )
}

/// User-message instructions for iterative re-compaction (a previous
/// summary exists in the first message of the history).
#[cfg(feature = "llm-compaction")]
fn structured_iterative_instructions(previous_summary: &str) -> String {
    format!(
        "You are updating a previous compaction summary. The first message in \
         the conversation above is a previous compaction's structured summary; \
         the remaining messages are new turns that need to be incorporated.\n\n\
         PREVIOUS SUMMARY:\n{previous_summary}\n\n\
         Update the summary using this exact structure. PRESERVE all existing \
         information that is still relevant. ADD new progress. Move items from \
         \"In Progress\" to \"Done\" when completed. Remove information only \
         if it is clearly obsolete.\n\n{STRUCTURED_TEMPLATE}\n\n\
         Target roughly 800 tokens. Be specific — include file paths, command \
         outputs, error messages, and concrete values. Write only the summary \
         body. Do not include any preamble or prefix."
    )
}

/// Extract a previous-compaction summary body from the first message of a
/// history slice, if it looks like one.
///
/// Only called by `structured_strategy`, which is feature-gated behind
/// `llm-compaction`. The `cfg_attr` keeps `--no-default-features` builds
/// from warning about the helper being unused.
#[cfg_attr(not(feature = "llm-compaction"), allow(dead_code))]
fn extract_previous_summary(history: &[Value]) -> Option<&str> {
    let first = history.first()?;
    if first.get("role").and_then(Value::as_str) != Some("user") {
        return None;
    }
    let content = first.get("content").and_then(Value::as_str)?;
    content.strip_prefix("[Conversation Summary]\n\n")
}

/// `CompactionMode::Structured` strategy — head + LLM summary + tail.
///
/// Same boundary logic as `recency_preserving_strategy`. The middle region
/// is summarised with a single LLM call using the Goal / Progress /
/// Decisions / Files / Next Steps template (see [`STRUCTURED_TEMPLATE`]).
/// Iterative re-compaction is supported: when the first head message is
/// already a compacted summary, the previous summary body is passed into
/// the prompt so the model can preserve and update its sections instead of
/// summarising from scratch.
///
/// On LLM failure, automatically falls back to
/// [`recency_preserving_strategy`] so compaction never silently drops
/// information — the retry history still has a middle marker and repaired
/// tool pairs even if the summary call failed.
///
/// Inspired by `hermes-agent`'s `ContextCompressor` and `openclaw`'s
/// `safeguard` compaction mode.
#[cfg(feature = "llm-compaction")]
async fn structured_strategy(
    history: &[Value],
    config: &CompactionConfig,
    context_window: u32,
    provider: &dyn LlmProvider,
) -> Result<Vec<Value>, CompactionRunError> {
    let bounds = compute_boundaries(history, config, context_window);
    let HeadTailBounds {
        head_end,
        tail_start,
        protect_tail_min,
        ..
    } = bounds;
    let n = history.len();

    // Head and tail already cover everything — no middle to summarise.
    if head_end >= tail_start {
        return in_place_prune_or_err(history, config, &bounds);
    }

    let middle = &history[head_end..tail_start];
    if middle.is_empty() {
        return in_place_prune_or_err(history, config, &bounds);
    }

    // Detect re-compaction: if the first head message is a previous
    // compaction summary, include it in the prompt so the model can update
    // sections instead of re-summarising from scratch.
    let previous_summary = extract_previous_summary(&history[..head_end]);

    // Build the structured prompt. System message frames the task, middle
    // messages are passed via ChatMessage so role boundaries are preserved
    // (prevents prompt injection via role prefixes in user content), and a
    // final user directive selects the template.
    let mut summary_messages = vec![ChatMessage::system(STRUCTURED_SYSTEM_INSTRUCTIONS)];
    summary_messages.extend(values_to_chat_messages(middle));
    summary_messages.push(match previous_summary {
        Some(prev) => ChatMessage::user(structured_iterative_instructions(prev)),
        None => ChatMessage::user(structured_first_compaction_instructions()),
    });

    // Stream the summary.
    let mut stream = provider.stream(summary_messages);
    let mut summary = String::new();
    let mut stream_error: Option<String> = None;
    while let Some(event) = stream.next().await {
        match event {
            StreamEvent::Delta(delta) => summary.push_str(&delta),
            StreamEvent::Done(_) => break,
            StreamEvent::Error(e) => {
                stream_error = Some(e.to_string());
                break;
            },
            // Tool events aren't expected on a summary stream; drop them.
            StreamEvent::ToolCallStart { .. }
            | StreamEvent::ToolCallArgumentsDelta { .. }
            | StreamEvent::ToolCallComplete { .. }
            // Provider raw payloads are debug metadata, not summary text.
            | StreamEvent::ProviderRaw(_)
            // Ignore reasoning blocks; the summary body is the final answer only.
            | StreamEvent::ReasoningDelta(_) => {},
        }
    }

    // `config.max_summary_tokens` / `config.summary_model` aren't wired
    // into the provider stream yet — tracked by beads issue moltis-8me.
    let _ = config.max_summary_tokens;
    let _ = config.summary_model.as_deref();

    if let Some(err) = stream_error {
        tracing::warn!(
            error = %err,
            "chat.compact: structured summary stream failed, falling back to recency_preserving"
        );
        return recency_preserving_strategy(history, config, context_window);
    }
    let summary = summary.trim();
    if summary.is_empty() {
        tracing::warn!(
            "chat.compact: structured summary was empty, falling back to recency_preserving"
        );
        return recency_preserving_strategy(history, config, context_window);
    }

    // Assemble head + structured-summary + tail.
    let mut kept: Vec<Value> = Vec::with_capacity(head_end + 1 + (n - tail_start));
    kept.extend(history[..head_end].iter().cloned());
    kept.push(build_summary_message(summary));
    kept.extend(history[tail_start..].iter().cloned());

    let kept = finalize_kept(kept, config, protect_tail_min)?;

    info!(
        input_messages = n,
        output_messages = kept.len(),
        head = head_end,
        middle = tail_start - head_end,
        tail = n - tail_start,
        summary_chars = summary.len(),
        iterative = previous_summary.is_some(),
        "chat.compact: structured"
    );

    Ok(kept)
}

/// `CompactionMode::LlmReplace` strategy — pre-PR #653 behaviour.
///
/// Streams a plain-text summary from the provider, then replaces the entire
/// history with a single user message containing it. Preserved for users who
/// explicitly want the old behaviour or need maximum token reduction.
#[cfg(feature = "llm-compaction")]
async fn llm_replace_strategy(
    history: &[Value],
    config: &CompactionConfig,
    provider: &dyn LlmProvider,
) -> Result<Vec<Value>, CompactionRunError> {
    // Build a structured prompt around the history so role boundaries are
    // maintained via the API's message structure. This prevents prompt
    // injection where user content could mimic role prefixes if we
    // concatenated everything into a single text blob.
    let mut summary_messages = vec![ChatMessage::system(
        "You are a conversation summarizer. The messages that follow are a \
         conversation you must summarize. Preserve all key facts, decisions, \
         and context. After the conversation, you will receive a final \
         instruction.",
    )];
    summary_messages.extend(values_to_chat_messages(history));
    summary_messages.push(ChatMessage::user(
        "Summarize the conversation above into a concise form. Output only \
         the summary, no preamble.",
    ));

    let mut stream = provider.stream(summary_messages);
    let mut summary = String::new();
    while let Some(event) = stream.next().await {
        match event {
            StreamEvent::Delta(delta) => summary.push_str(&delta),
            StreamEvent::Done(_) => break,
            StreamEvent::Error(e) => {
                return Err(CompactionRunError::LlmFailed(e.to_string()));
            },
            // Tool events aren't expected on a summary stream; drop them.
            StreamEvent::ToolCallStart { .. }
            | StreamEvent::ToolCallArgumentsDelta { .. }
            | StreamEvent::ToolCallComplete { .. }
            // Provider raw payloads are debug metadata, not summary text.
            | StreamEvent::ProviderRaw(_)
            // Ignore provider reasoning blocks; the summary body should only
            // include final answer text.
            | StreamEvent::ReasoningDelta(_) => {},
        }
    }

    // `config.summary_model` / `max_summary_tokens` aren't wired yet —
    // tracked by beads issue moltis-8me. Silence unused-field lint without
    // leaking that into the public API.
    let _ = config;

    if summary.is_empty() {
        return Err(CompactionRunError::EmptySummary);
    }

    info!(
        messages = history.len(),
        chars = summary.len(),
        "chat.compact: llm_replace summary"
    );

    Ok(vec![build_summary_message(&summary)])
}

// ── Recency-preserving helpers ────────────────────────────────────────────

/// Placeholder text injected when a bulky tool-result is pruned in place.
const PRUNED_TOOL_PLACEHOLDER: &str = "[Old tool output cleared to save context space]";

/// Rough token count for a persisted message.
///
/// Uses the same bytes/4 heuristic as the chat crate's existing estimator
/// plus a 10-token overhead for role/metadata framing. Covers the common
/// shapes without pulling in a tokenizer dependency.
fn message_tokens(message: &Value) -> u64 {
    const META_OVERHEAD: u64 = 10;
    let mut bytes: usize = 0;

    // Top-level content: string or array of content blocks.
    if let Some(text) = message.get("content").and_then(Value::as_str) {
        bytes += text.len();
    } else if let Some(blocks) = message.get("content").and_then(Value::as_array) {
        for block in blocks {
            if let Some(text) = block.get("text").and_then(Value::as_str) {
                bytes += text.len();
            } else if let Some(url) = block
                .get("image_url")
                .and_then(|iu| iu.get("url"))
                .and_then(Value::as_str)
            {
                bytes += url.len();
            }
        }
    }

    // Tool call arguments on assistant messages.
    if let Some(calls) = message.get("tool_calls").and_then(Value::as_array) {
        for call in calls {
            if let Some(args) = call
                .get("function")
                .and_then(|f| f.get("arguments"))
                .and_then(Value::as_str)
            {
                bytes += args.len();
            }
        }
    }

    // Tool-result structured fields.
    if let Some(result) = message.get("result") {
        bytes += serde_json::to_string(result).map(|s| s.len()).unwrap_or(0);
    }
    if let Some(error) = message.get("error").and_then(Value::as_str) {
        bytes += error.len();
    }

    ((bytes as u64) / 4) + META_OVERHEAD
}

/// True if the message is a tool/tool_result shape.
fn is_tool_role_value(message: &Value) -> bool {
    matches!(
        message.get("role").and_then(Value::as_str),
        Some("tool" | "tool_result")
    )
}

/// Extract tool-call IDs from an assistant message's `tool_calls` array.
fn assistant_tool_call_ids(message: &Value) -> Vec<String> {
    let Some(calls) = message.get("tool_calls").and_then(Value::as_array) else {
        return Vec::new();
    };
    calls
        .iter()
        .filter_map(|c| c.get("id").and_then(Value::as_str).map(str::to_string))
        .collect()
}

/// Extract `tool_call_id` from a tool/tool_result message.
fn tool_result_call_id(message: &Value) -> Option<&str> {
    message.get("tool_call_id").and_then(Value::as_str)
}

/// Walk forward past a consecutive run of tool/tool_result messages so the
/// tail boundary never starts mid-group.
fn align_boundary_forward_past_tool_group(history: &[Value], mut idx: usize) -> usize {
    while idx < history.len() && is_tool_role_value(&history[idx]) {
        idx += 1;
    }
    idx
}

/// Replace oversized tool-result content with [`PRUNED_TOOL_PLACEHOLDER`] in
/// messages before `end_exclusive`, returning the number of messages pruned.
///
/// Preserves lightweight tool results (under the threshold) and everything
/// at or after the protected tail region. Handles both the `role = "tool"`
/// shape (string `content`) and the `role = "tool_result"` shape
/// (`result` JSON + optional `error` string).
fn prune_tool_results_before(
    messages: &mut [Value],
    end_exclusive: usize,
    threshold_chars: u32,
) -> usize {
    let threshold = threshold_chars as usize;
    let mut pruned = 0;

    for msg in messages.iter_mut().take(end_exclusive) {
        if !is_tool_role_value(msg) {
            continue;
        }
        if prune_single_tool_result(msg, threshold) {
            pruned += 1;
        }
    }

    pruned
}

/// Replace oversized content on a single tool/tool_result message.
/// Returns `true` if anything was rewritten.
fn prune_single_tool_result(message: &mut Value, threshold: usize) -> bool {
    let mut changed = false;

    // `role = "tool"`: plain string content. Skip if already pruned or
    // under the threshold.
    if let Some(content) = message.get("content").and_then(Value::as_str) {
        if content == PRUNED_TOOL_PLACEHOLDER {
            return false;
        }
        if content.len() > threshold
            && let Some(obj) = message.as_object_mut()
        {
            obj.insert(
                "content".to_string(),
                Value::String(PRUNED_TOOL_PLACEHOLDER.to_string()),
            );
            changed = true;
        }
    }

    // `role = "tool_result"`: structured `result` + optional `error`.
    let result_too_big = message
        .get("result")
        .is_some_and(|r| serde_json::to_string(r).map(|s| s.len()).unwrap_or(0) > threshold);
    let error_too_big = message
        .get("error")
        .and_then(Value::as_str)
        .is_some_and(|e| e.len() > threshold);

    if (result_too_big || error_too_big)
        && let Some(obj) = message.as_object_mut()
    {
        if result_too_big {
            obj.insert(
                "result".to_string(),
                Value::String(PRUNED_TOOL_PLACEHOLDER.to_string()),
            );
            changed = true;
        }
        if error_too_big {
            obj.insert(
                "error".to_string(),
                Value::String(PRUNED_TOOL_PLACEHOLDER.to_string()),
            );
            changed = true;
        }
    }

    changed
}

/// Repair orphaned tool_call / tool_result pairs after compaction.
///
/// Two failure modes, both rejected by strict providers (Anthropic,
/// OpenAI strict mode):
///
/// 1. A tool result references a `tool_call_id` whose parent assistant
///    `tool_call` was dropped during pruning. → removed.
/// 2. An assistant `tool_call` has no matching tool result (the result was
///    dropped). → a stub tool result is inserted after the assistant
///    message so the pairing is well-formed.
///
/// Adapted from hermes-agent's `_sanitize_tool_pairs` and openclaw's
/// `repairToolUseResultPairing`.
fn sanitize_tool_pairs(messages: Vec<Value>) -> Result<Vec<Value>, CompactionRunError> {
    use std::collections::HashSet;

    // Pass 1: collect surviving tool_call IDs from assistant messages.
    let mut surviving_call_ids: HashSet<String> = HashSet::new();
    for msg in &messages {
        if msg.get("role").and_then(Value::as_str) == Some("assistant") {
            for id in assistant_tool_call_ids(msg) {
                surviving_call_ids.insert(id);
            }
        }
    }

    // Pass 2: collect the call IDs referenced by tool results.
    let mut result_call_ids: HashSet<String> = HashSet::new();
    for msg in &messages {
        if is_tool_role_value(msg)
            && let Some(id) = tool_result_call_id(msg)
        {
            result_call_ids.insert(id.to_string());
        }
    }

    // Pass 3: drop tool results whose call_id is no longer in the history.
    let orphaned: HashSet<String> = result_call_ids
        .difference(&surviving_call_ids)
        .cloned()
        .collect();
    let filtered: Vec<Value> = if orphaned.is_empty() {
        messages
    } else {
        messages
            .into_iter()
            .filter(|m| {
                if !is_tool_role_value(m) {
                    return true;
                }
                tool_result_call_id(m).is_none_or(|id| !orphaned.contains(id))
            })
            .collect()
    };

    // Pass 4: for every surviving assistant tool_call missing a matching
    // tool result, insert a stub tool message immediately after the parent.
    // We rebuild a new vec so we can splice stubs in the right positions.
    let mut patched: Vec<Value> = Vec::with_capacity(filtered.len());
    let mut satisfied: HashSet<String> = HashSet::new();

    // First pass to record which call IDs already have results further down
    // the history. We only need the count of surviving results here.
    for msg in &filtered {
        if is_tool_role_value(msg)
            && let Some(id) = tool_result_call_id(msg)
        {
            satisfied.insert(id.to_string());
        }
    }

    for msg in filtered {
        let is_assistant = msg.get("role").and_then(Value::as_str) == Some("assistant");
        let tool_calls = if is_assistant {
            assistant_tool_call_ids(&msg)
        } else {
            Vec::new()
        };
        patched.push(msg);
        for call_id in tool_calls {
            if !satisfied.contains(&call_id) {
                patched.push(stub_tool_result(&call_id));
                satisfied.insert(call_id);
            }
        }
    }

    Ok(patched)
}

/// Build a `role: tool` stub message for an orphaned assistant tool_call.
fn stub_tool_result(tool_call_id: &str) -> Value {
    let msg = PersistedMessage::Tool {
        tool_call_id: tool_call_id.to_string(),
        content: "[Result from earlier conversation — see context summary above]".to_string(),
        created_at: Some(crate::now_ms()),
    };
    msg.to_value()
}

/// Build a single user message that replaces the dropped middle region.
///
/// Counts each message by role so the LLM retry has a quick sense of what
/// was elided, then notes that recent turns are preserved verbatim below.
fn build_middle_marker(middle: &[Value]) -> Value {
    let mut users = 0usize;
    let mut assistants = 0usize;
    let mut tools = 0usize;
    for msg in middle {
        match msg.get("role").and_then(Value::as_str) {
            Some("user") => users += 1,
            Some("assistant") => assistants += 1,
            Some("tool") | Some("tool_result") => tools += 1,
            _ => {},
        }
    }

    let body = format!(
        "[Conversation Compacted]\n\n\
         {total} earlier messages were elided to save context space \
         ({users} user, {assistants} assistant, {tools} tool). \
         Recent messages are preserved verbatim below. \
         Use chat.compaction.mode = \"structured\" (when available) for a \
         full semantic summary of the omitted middle region.",
        total = middle.len(),
        users = users,
        assistants = assistants,
        tools = tools,
    );

    let msg = PersistedMessage::User {
        content: MessageContent::Text(body),
        created_at: Some(crate::now_ms()),
        audio: None,
        channel: None,
        seq: None,
        run_id: None,
    };
    msg.to_value()
}

/// Wrap a summary string in a `PersistedMessage::User` ready for `replace_history`.
///
/// Using the `user` role (not `assistant`) avoids breaking strict providers
/// (e.g. llama.cpp) that require every assistant message to follow a user
/// message, and keeps the summary in the conversation turn array for
/// providers using the Responses API (which promote system messages to
/// instructions and drop them from turns).
fn build_summary_message(body: &str) -> Value {
    let msg = PersistedMessage::User {
        content: MessageContent::Text(format!("[Conversation Summary]\n\n{body}")),
        created_at: Some(crate::now_ms()),
        audio: None,
        channel: None,
        seq: None,
        run_id: None,
    };
    msg.to_value()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use {super::*, serde_json::json};

    fn sample_history() -> Vec<Value> {
        vec![
            json!({"role": "user", "content": "hello"}),
            json!({"role": "assistant", "content": "hi there"}),
            json!({"role": "user", "content": "what is 2+2"}),
            json!({"role": "assistant", "content": "4"}),
        ]
    }

    #[tokio::test]
    async fn empty_history_returns_empty_history_error() {
        let config = CompactionConfig::default();
        let err = run_compaction(&[], &config, None).await.unwrap_err();
        assert!(matches!(err, CompactionRunError::EmptyHistory));
    }

    #[tokio::test]
    async fn deterministic_mode_returns_single_summary_message() {
        let history = sample_history();
        let config = CompactionConfig::default();
        let result = run_compaction(&history, &config, None).await.unwrap();
        assert_eq!(
            result.len(),
            1,
            "deterministic mode replaces history with one message"
        );
        let text = result[0]
            .get("content")
            .and_then(Value::as_str)
            .expect("summary has string content");
        assert!(
            text.starts_with("[Conversation Summary]\n\n"),
            "summary is wrapped in the expected preamble, got: {text}"
        );
    }

    // ── RecencyPreserving strategy ────────────────────────────────────

    fn mk_user(text: &str) -> Value {
        json!({"role": "user", "content": text})
    }

    fn mk_assistant(text: &str) -> Value {
        json!({"role": "assistant", "content": text})
    }

    fn mk_assistant_with_tool_call(text: &str, call_id: &str, tool: &str) -> Value {
        json!({
            "role": "assistant",
            "content": text,
            "tool_calls": [{
                "id": call_id,
                "type": "function",
                "function": { "name": tool, "arguments": "{}" }
            }]
        })
    }

    fn mk_tool_result(call_id: &str, content: &str) -> Value {
        json!({
            "role": "tool",
            "tool_call_id": call_id,
            "content": content,
        })
    }

    fn config_with_small_boundaries() -> CompactionConfig {
        CompactionConfig {
            mode: CompactionMode::RecencyPreserving,
            protect_head: 2,
            protect_tail_min: 2,
            tool_prune_char_threshold: 20,
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn recency_preserving_tiny_history_returns_too_small_error() {
        // A 4-message sample with no tool-result content is below the
        // 3+20 default floor and has nothing to prune — expect a clear
        // error pointing the user at a different mode.
        let history = sample_history();
        let config = CompactionConfig {
            mode: CompactionMode::RecencyPreserving,
            ..Default::default()
        };
        let err = run_compaction(&history, &config, None).await.unwrap_err();
        match err {
            CompactionRunError::TooSmallToCompact { messages, .. } => {
                assert_eq!(messages, history.len());
            },
            other => panic!("expected TooSmallToCompact, got {other:?}"),
        }
    }

    /// Context window small enough to force the tail budget to consume only
    /// a handful of messages in tests. With default `threshold_percent=0.75`
    /// and `tail_budget_ratio=0.20`, 200 tokens of budget means
    /// `200 × 0.75 × 0.20 = 30` tokens — roughly two small messages after
    /// the 10-token metadata overhead per message.
    const TEST_CONTEXT_WINDOW_TINY: u32 = 200;

    #[test]
    fn recency_preserving_splices_marker_between_head_and_tail() {
        // 10 messages: head=2, tail=2 → middle=6 collapsed into 1 marker.
        let mut history = Vec::new();
        for i in 0..5 {
            history.push(mk_user(&format!("user {i}")));
            history.push(mk_assistant(&format!("assistant {i}")));
        }

        let config = config_with_small_boundaries();
        let result =
            recency_preserving_strategy(&history, &config, TEST_CONTEXT_WINDOW_TINY).unwrap();

        // 2 head + 1 marker + 2 tail = 5 messages.
        assert_eq!(result.len(), 5, "result: {result:#?}");

        // Head is verbatim.
        assert_eq!(
            result[0].get("content").and_then(Value::as_str),
            Some("user 0")
        );
        assert_eq!(
            result[1].get("content").and_then(Value::as_str),
            Some("assistant 0")
        );

        // Marker has the right shape.
        let marker = result[2]
            .get("content")
            .and_then(Value::as_str)
            .expect("marker content");
        assert!(
            marker.starts_with("[Conversation Compacted]"),
            "got: {marker}"
        );
        assert!(marker.contains("6 earlier messages"), "got: {marker}");

        // Tail is verbatim — the LAST two source messages.
        assert_eq!(
            result[3].get("content").and_then(Value::as_str),
            Some("user 4")
        );
        assert_eq!(
            result[4].get("content").and_then(Value::as_str),
            Some("assistant 4")
        );
    }

    #[tokio::test]
    async fn recency_preserving_prunes_oversized_tool_results_in_head_only_session() {
        // Only 3 messages total — head+tail cover everything, but the
        // middle tool output is bulky enough to be worth pruning in place.
        let oversized = "x".repeat(500);
        let history = vec![
            mk_user("first user"),
            mk_assistant_with_tool_call("calling tool", "call_1", "read_file"),
            mk_tool_result("call_1", &oversized),
        ];

        let config = CompactionConfig {
            mode: CompactionMode::RecencyPreserving,
            protect_head: 3,
            protect_tail_min: 3,
            tool_prune_char_threshold: 20,
            ..Default::default()
        };
        let result = run_compaction(&history, &config, None).await.unwrap();

        // Same number of messages, but the tool content is now a placeholder.
        assert_eq!(result.len(), 3);
        let tool_content = result[2]
            .get("content")
            .and_then(Value::as_str)
            .expect("tool content");
        assert_eq!(tool_content, PRUNED_TOOL_PLACEHOLDER);
    }

    #[test]
    fn recency_preserving_drops_orphaned_tool_results() {
        // Head keeps first 2 messages. A later tool result references a
        // parent that lives in the dropped middle — it must be removed so
        // strict providers don't reject the retry. Use an explicit small
        // context window so the middle cut actually happens.
        let history = vec![
            mk_user("u0"),
            mk_assistant("a0"),
            // Middle starts here — these will be collapsed into the marker.
            mk_user("u1"),
            mk_assistant_with_tool_call("mid-a", "orphan_call", "exec"),
            mk_tool_result("orphan_call", "mid tool out"),
            mk_user("u2"),
            mk_assistant("mid-a2"),
            // Another orphan result that appears in the tail but whose
            // parent assistant was in the middle and got dropped.
            mk_tool_result("orphan_call", "late tail out"),
            mk_user("u3"),
        ];

        let config = CompactionConfig {
            mode: CompactionMode::RecencyPreserving,
            protect_head: 2,
            protect_tail_min: 2,
            // High enough that nothing in the kept slice gets pruned in place.
            tool_prune_char_threshold: 10_000,
            ..Default::default()
        };
        let result =
            recency_preserving_strategy(&history, &config, TEST_CONTEXT_WINDOW_TINY).unwrap();

        // The orphaned tool result must not survive anywhere.
        for msg in &result {
            if is_tool_role_value(msg) {
                assert_ne!(
                    tool_result_call_id(msg),
                    Some("orphan_call"),
                    "orphaned tool result should be dropped, got: {msg:#?}"
                );
            }
        }
    }

    #[test]
    fn recency_preserving_stubs_missing_tool_results_for_surviving_assistant_calls() {
        // Assistant with tool_call survives in the head; its tool result is
        // in the middle and gets dropped. Sanitizer must insert a stub so
        // the call_id is satisfied.
        let history = vec![
            mk_user("start"),
            mk_assistant_with_tool_call("running", "head_call", "exec"),
            // Middle: tool result for head_call is here and will be elided.
            mk_tool_result("head_call", "result body"),
            mk_user("filler 1"),
            mk_assistant("filler reply 1"),
            mk_user("filler 2"),
            mk_assistant("filler reply 2"),
            // Tail: unrelated messages.
            mk_user("tail user"),
            mk_assistant("tail assistant"),
        ];

        let config = CompactionConfig {
            mode: CompactionMode::RecencyPreserving,
            protect_head: 2,
            protect_tail_min: 2,
            tool_prune_char_threshold: 10_000,
            ..Default::default()
        };
        let result =
            recency_preserving_strategy(&history, &config, TEST_CONTEXT_WINDOW_TINY).unwrap();

        // The head assistant's tool_call must have a matching result somewhere.
        let stub = result
            .iter()
            .find(|m| is_tool_role_value(m) && tool_result_call_id(m) == Some("head_call"));
        assert!(
            stub.is_some(),
            "expected a stub tool result for head_call, got: {result:#?}"
        );
    }

    #[cfg(feature = "llm-compaction")]
    #[tokio::test]
    async fn structured_mode_without_provider_returns_provider_required() {
        let history = sample_history();
        let config = CompactionConfig {
            mode: CompactionMode::Structured,
            ..Default::default()
        };
        let err = run_compaction(&history, &config, None).await.unwrap_err();
        match err {
            CompactionRunError::ProviderRequired { mode } => assert_eq!(mode, "structured"),
            other => panic!("expected ProviderRequired, got {other:?}"),
        }
    }

    #[cfg(not(feature = "llm-compaction"))]
    #[tokio::test]
    async fn structured_mode_returns_feature_disabled_when_feature_off() {
        let history = sample_history();
        let config = CompactionConfig {
            mode: CompactionMode::Structured,
            ..Default::default()
        };
        let err = run_compaction(&history, &config, None).await.unwrap_err();
        match err {
            CompactionRunError::FeatureDisabled { mode } => assert_eq!(mode, "structured"),
            other => panic!("expected FeatureDisabled, got {other:?}"),
        }
    }

    #[cfg(feature = "llm-compaction")]
    #[tokio::test]
    async fn llm_replace_mode_without_provider_returns_provider_required() {
        let history = sample_history();
        let config = CompactionConfig {
            mode: CompactionMode::LlmReplace,
            ..Default::default()
        };
        let err = run_compaction(&history, &config, None).await.unwrap_err();
        match err {
            CompactionRunError::ProviderRequired { mode } => {
                assert_eq!(mode, "llm_replace");
            },
            other => panic!("expected ProviderRequired, got {other:?}"),
        }
    }

    #[cfg(not(feature = "llm-compaction"))]
    #[tokio::test]
    async fn llm_replace_mode_returns_feature_disabled_when_feature_off() {
        let history = sample_history();
        let config = CompactionConfig {
            mode: CompactionMode::LlmReplace,
            ..Default::default()
        };
        let err = run_compaction(&history, &config, None).await.unwrap_err();
        match err {
            CompactionRunError::FeatureDisabled { mode } => {
                assert_eq!(mode, "llm_replace");
            },
            other => panic!("expected FeatureDisabled, got {other:?}"),
        }
    }

    // ── LLM-backed modes with stub providers ──────────────────────────

    #[cfg(feature = "llm-compaction")]
    mod stub_provider {
        use {
            super::*,
            anyhow::Result,
            async_trait::async_trait,
            futures::Stream,
            moltis_agents::model::{CompletionResponse, Usage, UserContent},
            std::{
                pin::Pin,
                sync::{Arc, Mutex},
            },
        };

        /// Stub provider that emits a canned sequence of stream events.
        ///
        /// `context_window` lets the caller force the tail-budget math into
        /// the cutting regime; `events` is the full sequence returned by
        /// `stream()` on every call. When `needle` is set, the provider
        /// records whether any text field in the received `messages`
        /// contains the needle — used to assert that iterative
        /// re-compaction forwards the previous summary body into the
        /// prompt.
        pub(super) struct StubProvider {
            pub events: Vec<StreamEvent>,
            pub context_window: u32,
            pub needle: Option<String>,
            pub saw_needle: Arc<Mutex<bool>>,
        }

        impl StubProvider {
            pub fn new_ok(body: &str) -> Self {
                Self {
                    events: vec![
                        StreamEvent::Delta(body.to_string()),
                        StreamEvent::Done(Usage::default()),
                    ],
                    context_window: 200,
                    needle: None,
                    saw_needle: Arc::new(Mutex::new(false)),
                }
            }

            pub fn new_error(msg: &str) -> Self {
                Self {
                    events: vec![StreamEvent::Error(msg.to_string())],
                    context_window: 200,
                    needle: None,
                    saw_needle: Arc::new(Mutex::new(false)),
                }
            }

            pub fn with_needle(mut self, needle: impl Into<String>) -> Self {
                self.needle = Some(needle.into());
                self
            }

            pub fn saw_needle(&self) -> bool {
                *self
                    .saw_needle
                    .lock()
                    .expect("stub provider mutex poisoned")
            }
        }

        fn message_contains(msg: &ChatMessage, needle: &str) -> bool {
            match msg {
                ChatMessage::System { content } => content.contains(needle),
                ChatMessage::User {
                    content: UserContent::Text(t),
                } => t.contains(needle),
                ChatMessage::User {
                    content: UserContent::Multimodal(parts),
                } => parts.iter().any(|p| {
                    matches!(p, moltis_agents::model::ContentPart::Text(t) if t.contains(needle))
                }),
                ChatMessage::Assistant {
                    content: Some(text),
                    ..
                } => text.contains(needle),
                ChatMessage::Tool { content, .. } => content.contains(needle),
                _ => false,
            }
        }

        #[async_trait]
        impl LlmProvider for StubProvider {
            fn name(&self) -> &str {
                "stub"
            }

            fn id(&self) -> &str {
                "stub::compaction"
            }

            fn context_window(&self) -> u32 {
                self.context_window
            }

            async fn complete(
                &self,
                _messages: &[ChatMessage],
                _tools: &[Value],
            ) -> Result<CompletionResponse> {
                anyhow::bail!("stub does not implement complete")
            }

            fn stream(
                &self,
                messages: Vec<ChatMessage>,
            ) -> Pin<Box<dyn Stream<Item = StreamEvent> + Send + '_>> {
                if let Some(needle) = &self.needle
                    && messages.iter().any(|m| message_contains(m, needle))
                {
                    *self
                        .saw_needle
                        .lock()
                        .expect("stub provider mutex poisoned") = true;
                }
                let events = self.events.clone();
                Box::pin(tokio_stream::iter(events))
            }
        }
    }

    #[cfg(feature = "llm-compaction")]
    #[tokio::test]
    async fn llm_replace_mode_with_stub_provider_returns_single_message() {
        let history = sample_history();
        let config = CompactionConfig {
            mode: CompactionMode::LlmReplace,
            ..Default::default()
        };
        let provider = stub_provider::StubProvider::new_ok("stubbed summary body");
        let result = run_compaction(&history, &config, Some(&provider))
            .await
            .expect("llm_replace succeeds with stub provider");
        assert_eq!(result.len(), 1);
        let text = result[0]
            .get("content")
            .and_then(Value::as_str)
            .expect("summary content");
        assert!(text.contains("stubbed summary body"), "got: {text}");
    }

    #[cfg(feature = "llm-compaction")]
    #[tokio::test]
    async fn structured_mode_splices_summary_between_head_and_tail() {
        // 10 messages so the boundaries cut in the middle with a tiny
        // context window. Stub provider returns a well-formed structured
        // summary body and the strategy should preserve head/tail verbatim
        // around it.
        let mut history = Vec::new();
        for i in 0..5 {
            history.push(mk_user(&format!("user {i}")));
            history.push(mk_assistant(&format!("assistant {i}")));
        }

        let config = CompactionConfig {
            mode: CompactionMode::Structured,
            protect_head: 2,
            protect_tail_min: 2,
            ..Default::default()
        };
        let provider = stub_provider::StubProvider::new_ok(
            "## Goal\nTest compaction\n## Progress\n### Done\nAll the things",
        );
        let result = run_compaction(&history, &config, Some(&provider))
            .await
            .expect("structured succeeds with stub provider");

        // Head (2) + structured summary (1) + tail (2) = 5 messages.
        assert_eq!(result.len(), 5, "result: {result:#?}");

        assert_eq!(
            result[0].get("content").and_then(Value::as_str),
            Some("user 0")
        );
        assert_eq!(
            result[1].get("content").and_then(Value::as_str),
            Some("assistant 0")
        );

        let summary = result[2]
            .get("content")
            .and_then(Value::as_str)
            .expect("summary");
        assert!(
            summary.starts_with("[Conversation Summary]\n\n"),
            "got: {summary}"
        );
        assert!(summary.contains("## Goal"), "got: {summary}");

        assert_eq!(
            result[3].get("content").and_then(Value::as_str),
            Some("user 4")
        );
        assert_eq!(
            result[4].get("content").and_then(Value::as_str),
            Some("assistant 4")
        );
    }

    #[cfg(feature = "llm-compaction")]
    #[tokio::test]
    async fn structured_mode_falls_back_to_recency_preserving_on_llm_error() {
        let mut history = Vec::new();
        for i in 0..5 {
            history.push(mk_user(&format!("user {i}")));
            history.push(mk_assistant(&format!("assistant {i}")));
        }

        let config = CompactionConfig {
            mode: CompactionMode::Structured,
            protect_head: 2,
            protect_tail_min: 2,
            ..Default::default()
        };
        let provider = stub_provider::StubProvider::new_error("simulated provider outage");
        let result = run_compaction(&history, &config, Some(&provider))
            .await
            .expect("structured falls back to recency_preserving on llm error");

        // Fallback produces a recency_preserving-shaped history: head (2) +
        // middle marker (1) + tail (2) = 5 messages, and the middle message
        // is the plain "[Conversation Compacted]" marker, not a structured
        // summary.
        assert_eq!(result.len(), 5, "result: {result:#?}");
        let middle = result[2]
            .get("content")
            .and_then(Value::as_str)
            .expect("middle content");
        assert!(
            middle.starts_with("[Conversation Compacted]"),
            "fallback should produce the recency_preserving marker, got: {middle}"
        );
    }

    #[cfg(feature = "llm-compaction")]
    #[tokio::test]
    async fn structured_mode_forwards_previous_summary_on_recompaction() {
        // First head message is a previous compaction summary. The stub
        // provider captures whether any forwarded message contains the
        // unique needle from that prior body, verifying that the
        // iterative-compaction prompt actually reaches the provider.
        const NEEDLE: &str = "previous-compaction-needle-a1b2c3";
        let prior = format!("[Conversation Summary]\n\n## Goal\n{NEEDLE}");
        let mut history = vec![
            json!({"role": "user", "content": prior}),
            mk_assistant("ok got it"),
        ];
        for i in 0..5 {
            history.push(mk_user(&format!("user {i}")));
            history.push(mk_assistant(&format!("assistant {i}")));
        }

        let config = CompactionConfig {
            mode: CompactionMode::Structured,
            protect_head: 2,
            protect_tail_min: 2,
            ..Default::default()
        };
        let provider =
            stub_provider::StubProvider::new_ok("## Goal\nstub output").with_needle(NEEDLE);
        let _ = run_compaction(&history, &config, Some(&provider))
            .await
            .expect("structured succeeds with stub provider");

        assert!(
            provider.saw_needle(),
            "structured mode must forward the previous summary body into the iterative-compaction prompt"
        );
    }

    #[cfg(feature = "llm-compaction")]
    #[tokio::test]
    async fn structured_mode_falls_back_when_summary_is_empty() {
        // A stream that yields Done with no Delta should surface as an
        // empty summary and trigger the same fallback path as an error.
        use moltis_agents::model::Usage;
        let mut history = Vec::new();
        for i in 0..5 {
            history.push(mk_user(&format!("user {i}")));
            history.push(mk_assistant(&format!("assistant {i}")));
        }

        let config = CompactionConfig {
            mode: CompactionMode::Structured,
            protect_head: 2,
            protect_tail_min: 2,
            ..Default::default()
        };
        let provider = stub_provider::StubProvider {
            events: vec![StreamEvent::Done(Usage::default())],
            context_window: 200,
            needle: None,
            saw_needle: std::sync::Arc::new(std::sync::Mutex::new(false)),
        };
        let result = run_compaction(&history, &config, Some(&provider))
            .await
            .expect("structured falls back on empty summary");
        assert_eq!(result.len(), 5);
        let middle = result[2]
            .get("content")
            .and_then(Value::as_str)
            .expect("middle content");
        assert!(
            middle.starts_with("[Conversation Compacted]"),
            "expected fallback marker, got: {middle}"
        );
    }

    #[cfg(feature = "llm-compaction")]
    #[test]
    fn extract_previous_summary_detects_compacted_head() {
        let history = vec![json!({
            "role": "user",
            "content": "[Conversation Summary]\n\n## Goal\nprior goal",
        })];
        assert_eq!(
            extract_previous_summary(&history),
            Some("## Goal\nprior goal")
        );

        let not_compacted = vec![json!({"role": "user", "content": "hello"})];
        assert_eq!(extract_previous_summary(&not_compacted), None);

        let empty: Vec<Value> = Vec::new();
        assert_eq!(extract_previous_summary(&empty), None);
    }

    // ── extract_summary_body (memory-file / hook helper) ──────────────

    #[test]
    fn extract_summary_body_finds_conversation_summary_prefix() {
        // deterministic / structured / llm_replace shape: single summary
        // message at index 0.
        let compacted = vec![json!({
            "role": "user",
            "content": "[Conversation Summary]\n\nBody text here",
        })];
        assert_eq!(extract_summary_body(&compacted), "Body text here");
    }

    #[test]
    fn extract_summary_body_finds_conversation_compacted_marker() {
        // recency_preserving shape: head verbatim, then middle marker,
        // then tail verbatim. The marker is NOT at index 0.
        let compacted = vec![
            json!({"role": "user", "content": "first user"}),
            json!({"role": "assistant", "content": "first reply"}),
            json!({
                "role": "user",
                "content": "[Conversation Compacted]\n\n6 earlier messages were elided …",
            }),
            json!({"role": "user", "content": "recent user"}),
            json!({"role": "assistant", "content": "recent reply"}),
        ];
        let body = extract_summary_body(&compacted);
        assert!(
            body.starts_with("6 earlier messages were elided"),
            "got: {body}"
        );
    }

    #[test]
    fn extract_summary_body_returns_empty_when_no_summary_shaped_message_present() {
        // Pathological: history with no summary-shaped message. Helper
        // should return "" rather than picking up unrelated content.
        let compacted = vec![
            json!({"role": "user", "content": "just a regular user turn"}),
            json!({"role": "assistant", "content": "just a regular reply"}),
        ];
        assert_eq!(extract_summary_body(&compacted), "");
    }
}
