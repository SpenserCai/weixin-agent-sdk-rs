//! Per-run outbound handle: `run_id` propagation and tool-call progress.

use std::path::Path;
use std::sync::Arc;

use crate::error::Result;
use crate::messaging::inbound::SendResult;
use crate::messaging::sender::MessageSender;
use crate::types::{
    MessageItem, MessageItemType, ToolCallResultItem, ToolCallStartItem, ToolCallStatus,
};
use crate::util::{now_ms_i64, random::generate_run_id};

/// A scoped handle for one logical outbound run.
///
/// All messages sent through this handle carry the same `run_id`, which lets the
/// peer group them as one run. The run boundary is defined by the caller — this
/// SDK never infers it.
///
/// Obtain one from [`crate::MessageContext::run`] or [`crate::WeixinClient::run`].
///
/// # Ordering
///
/// Each send is one HTTP request. Awaiting calls in sequence guarantees the peer
/// observes them in that order. If you spawn sends concurrently, ordering is
/// yours to manage.
///
/// # Observed behaviour
///
/// Verified against the production API on 2026-08-13: the server accepts
/// tool-call progress items (HTTP 200) but answers with an empty body and
/// allocates no `message_id`, whereas any message carrying a text item does get
/// one — the server does not treat progress items as conversation messages, and
/// the `WeChat` client does not render them. Five wire variants were tried
/// (`GENERATING` state, progress merged into a text `item_list`, added
/// `msg_id`/`update_time_ms`, dropped `run_id`/`context_token`) with no change,
/// and `getConfig` exposes no capability flag. This reads as a capability iLink
/// has reserved but not yet enabled.
///
/// `run_id` was likewise not observed to affect client-side presentation; treat
/// it as a server-side correlation field. The value of both features today is
/// protocol readiness, not a user-visible progress display.
///
/// # Example
///
/// ```rust,no_run
/// # use weixin_agent::{MessageContext, Result, ToolCallStatus};
/// # async fn demo(ctx: &MessageContext) -> Result<()> {
/// let run = ctx.run();
/// run.tool_call_start("bash", Some("call-1")).await?;
/// // ... execute the tool ...
/// run.tool_call_result("bash", Some("call-1"), ToolCallStatus::Completed)
///     .await?;
/// run.send_text("done").await?;
/// # Ok(())
/// # }
/// ```
pub struct OutboundRun {
    sender: Arc<MessageSender>,
    to: String,
    context_token: Option<String>,
    run_id: String,
}

impl OutboundRun {
    /// Create a run with a freshly generated run ID.
    pub(crate) fn new(sender: Arc<MessageSender>, to: &str, context_token: Option<&str>) -> Self {
        Self {
            sender,
            to: to.to_owned(),
            context_token: context_token.map(String::from),
            run_id: generate_run_id(),
        }
    }

    /// Override the auto-generated run ID (e.g. to reuse a caller-side run identifier).
    #[must_use]
    pub fn with_run_id(mut self, run_id: impl Into<String>) -> Self {
        self.run_id = run_id.into();
        self
    }

    /// The run ID carried by every message sent through this handle.
    pub fn run_id(&self) -> &str {
        &self.run_id
    }

    /// The recipient of this run.
    pub fn to(&self) -> &str {
        &self.to
    }

    /// Send text (markdown filter applies per config, same as `reply_text`).
    pub async fn send_text(&self, text: &str) -> Result<SendResult> {
        self.sender
            .send_text(
                &self.to,
                text,
                self.context_token.as_deref(),
                Some(&self.run_id),
            )
            .await
    }

    /// Upload and send a media file.
    pub async fn send_media(&self, file_path: &Path) -> Result<SendResult> {
        self.sender
            .send_media(
                &self.to,
                file_path,
                self.context_token.as_deref(),
                Some(&self.run_id),
            )
            .await
    }

    /// Announce that a tool call started.
    ///
    /// `tool_call_id` pairs this event with the matching
    /// [`Self::tool_call_result`]; provide one whenever the peer may see
    /// overlapping calls.
    pub async fn tool_call_start(
        &self,
        tool_name: &str,
        tool_call_id: Option<&str>,
    ) -> Result<SendResult> {
        self.sender
            .send_item(
                &self.to,
                build_tool_call_start_item(tool_name, tool_call_id),
                self.context_token.as_deref(),
                Some(&self.run_id),
            )
            .await
    }

    /// Announce that a tool call finished.
    pub async fn tool_call_result(
        &self,
        tool_name: &str,
        tool_call_id: Option<&str>,
        status: ToolCallStatus,
    ) -> Result<SendResult> {
        self.sender
            .send_item(
                &self.to,
                build_tool_call_result_item(tool_name, tool_call_id, status),
                self.context_token.as_deref(),
                Some(&self.run_id),
            )
            .await
    }
}

/// Build a tool-call start item (type 11, not yet completed).
///
/// The markdown filter is deliberately not applied: `tool_name` is an identifier,
/// not display prose.
pub(crate) fn build_tool_call_start_item(
    tool_name: &str,
    tool_call_id: Option<&str>,
) -> MessageItem {
    MessageItem {
        item_type: Some(MessageItemType::ToolCallStart),
        create_time_ms: Some(now_ms_i64()),
        is_completed: Some(false),
        tool_call_start_item: Some(ToolCallStartItem {
            tool_name: Some(tool_name.to_owned()),
            tool_call_id: tool_call_id.map(String::from),
        }),
        ..Default::default()
    }
}

/// Build a tool-call result item (type 12, completed).
pub(crate) fn build_tool_call_result_item(
    tool_name: &str,
    tool_call_id: Option<&str>,
    status: ToolCallStatus,
) -> MessageItem {
    MessageItem {
        item_type: Some(MessageItemType::ToolCallResult),
        create_time_ms: Some(now_ms_i64()),
        is_completed: Some(true),
        tool_call_result_item: Some(ToolCallResultItem {
            tool_name: Some(tool_name.to_owned()),
            tool_call_id: tool_call_id.map(String::from),
            status: Some(status.as_str().to_owned()),
        }),
        ..Default::default()
    }
}
