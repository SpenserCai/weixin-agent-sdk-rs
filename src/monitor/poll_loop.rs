//! Long-poll `getUpdates` loop with error handling, backoff, and cooldown guard.

use std::sync::Arc;
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use crate::api::client::HttpApiClient;
use crate::api::session_guard::SessionGuard;
use crate::error::Result;
use crate::messaging::inbound::{self, ContextTokenStore};
use crate::messaging::sender::MessageSender;
use crate::types::{
    BACKOFF_DELAY_MS, GetUpdatesRequest, MAX_CONSECUTIVE_FAILURES, RETRY_DELAY_MS,
    STALE_TOKEN_ERRCODE,
};
use crate::util::net_error;

/// Details of a stale-token event.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct TokenStaleInfo {
    /// Error code reported by the server (`-14`).
    pub errcode: i32,
    /// How long the SDK will pause polling before trying again.
    pub pause_duration: Duration,
}

/// The handler trait users implement to receive messages.
#[async_trait::async_trait]
pub trait MessageHandler: Send + Sync {
    /// Called for each inbound user message.
    async fn on_message(&self, ctx: &inbound::MessageContext) -> Result<()>;

    /// Called when `get_updates_buf` changes — persist it here.
    async fn on_sync_buf_updated(&self, _sync_buf: &str) -> Result<()> {
        Ok(())
    }

    /// Called when the server reports that the bot token is stale.
    ///
    /// The SDK then pauses the poll loop for the cooldown window. Persist the
    /// account state here (e.g. mark it as needing re-authentication). Errors
    /// returned from this hook are logged and do not stop the loop.
    ///
    /// To stop the loop immediately, hold a `CancellationToken` shared with the
    /// client via [`crate::WeixinClientBuilder::with_cancel_token`] and cancel it
    /// here — the handler is moved into the builder, so it cannot hold the
    /// [`crate::WeixinClient`] itself.
    async fn on_token_stale(&self, _info: &TokenStaleInfo) -> Result<()> {
        Ok(())
    }

    /// Lifecycle hook: called before the poll loop starts.
    async fn on_start(&self) -> Result<()> {
        Ok(())
    }

    /// Lifecycle hook: called after the poll loop ends.
    async fn on_shutdown(&self) -> Result<()> {
        Ok(())
    }
}

/// Run the long-poll monitor loop. Blocks until cancellation.
// 8 arguments: the outbound engine, guard, token store, and cancellation token are
// independent collaborators owned by `WeixinClient`; bundling them into a struct
// would only move the same list one level down.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub(crate) async fn run_monitor(
    api: Arc<HttpApiClient>,
    sender: Arc<MessageSender>,
    handler: Arc<dyn MessageHandler>,
    session_guard: Arc<SessionGuard>,
    context_tokens: Arc<ContextTokenStore>,
    initial_sync_buf: Option<String>,
    initial_timeout: Duration,
    cancel: CancellationToken,
) -> Result<()> {
    handler.on_start().await?;

    let mut get_updates_buf = initial_sync_buf.unwrap_or_default();
    let mut next_timeout = initial_timeout;
    let mut consecutive_failures: u32 = 0;

    loop {
        if cancel.is_cancelled() {
            break;
        }

        // Check the cooldown guard
        if session_guard.is_paused() {
            let remaining = session_guard.remaining_ms();
            tracing::info!(remaining_ms = remaining, "poll loop paused, sleeping");
            tokio::select! {
                () = cancel.cancelled() => break,
                () = tokio::time::sleep(Duration::from_millis(remaining)) => continue,
            }
        }

        let req = GetUpdatesRequest {
            get_updates_buf: get_updates_buf.clone(),
            base_info: api.base_info(),
        };

        let resp = tokio::select! {
            () = cancel.cancelled() => break,
            result = api.get_updates(&req, next_timeout) => {
                match result {
                    Ok(r) => r,
                    Err(e) => {
                        consecutive_failures += 1;
                        let kind = net_error::classify(&e);
                        // The error text is not logged: it can carry an un-redacted
                        // URL with its query string (standards §1.3).
                        tracing::error!(
                            kind = kind.as_str(),
                            description = kind.description(),
                            failures = consecutive_failures,
                            "getUpdates transport failure"
                        );
                        if consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
                            consecutive_failures = 0;
                            sleep_or_cancel(Duration::from_millis(BACKOFF_DELAY_MS), &cancel).await;
                        } else {
                            sleep_or_cancel(Duration::from_millis(RETRY_DELAY_MS), &cancel).await;
                        }
                        continue;
                    }
                }
            }
        };

        // Update dynamic timeout
        if let Some(t) = resp.longpolling_timeout_ms {
            if t > 0 {
                next_timeout = Duration::from_millis(t);
            }
        }

        // Check API-level errors
        let is_error = resp.ret.unwrap_or(0) != 0 || resp.errcode.unwrap_or(0) != 0;
        if is_error {
            let errcode = resp.errcode.or(resp.ret).unwrap_or(0);
            if errcode == STALE_TOKEN_ERRCODE {
                session_guard.pause();
                consecutive_failures = 0;
                let remaining = session_guard.remaining_ms();
                tracing::error!(
                    errcode,
                    pause_min = remaining / 60_000,
                    "bot token is stale, pausing poll loop"
                );
                let info = TokenStaleInfo {
                    errcode,
                    pause_duration: Duration::from_millis(remaining),
                };
                if let Err(e) = handler.on_token_stale(&info).await {
                    tracing::error!(error = %e, "on_token_stale failed");
                }
                sleep_or_cancel(Duration::from_millis(remaining), &cancel).await;
                continue;
            }

            consecutive_failures += 1;
            tracing::error!(
                ret = resp.ret,
                errcode = resp.errcode,
                errmsg = resp.errmsg.as_deref().unwrap_or(""),
                failures = consecutive_failures,
                "getUpdates API error"
            );
            if consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
                consecutive_failures = 0;
                sleep_or_cancel(Duration::from_millis(BACKOFF_DELAY_MS), &cancel).await;
            } else {
                sleep_or_cancel(Duration::from_millis(RETRY_DELAY_MS), &cancel).await;
            }
            continue;
        }

        // Success
        consecutive_failures = 0;

        // Update sync buf (prefer get_updates_buf, fall back to deprecated sync_buf)
        let new_buf = resp
            .get_updates_buf
            .as_ref()
            .or(resp.sync_buf.as_ref())
            .filter(|b| !b.is_empty());
        if let Some(new_buf) = new_buf {
            get_updates_buf.clone_from(new_buf);
            if let Err(e) = handler.on_sync_buf_updated(new_buf).await {
                tracing::error!(error = %e, "on_sync_buf_updated failed");
            }
        }

        // Process messages
        let msgs = resp.msgs.unwrap_or_default();
        for msg in &msgs {
            if !inbound::should_process(msg) {
                continue;
            }

            // Update context token store
            if let (Some(from), Some(token)) = (&msg.from_user_id, &msg.context_token) {
                context_tokens.set(from, token);
            }

            let ctx = inbound::parse_inbound_message(msg, Arc::clone(&sender));
            if let Err(e) = handler.on_message(&ctx).await {
                tracing::error!(
                    error = %e,
                    from = %ctx.from,
                    message_id = %ctx.message_id,
                    "on_message handler error"
                );
            }
        }
    }

    if let Err(e) = api.notify_stop().await {
        // Best-effort; the classified transport failure is already logged by the
        // API client, and the error text may carry an un-redacted URL.
        tracing::warn!(
            kind = net_error::classify(&e).as_str(),
            "notify_stop failed"
        );
    }

    handler.on_shutdown().await?;
    tracing::info!("monitor loop ended");
    Ok(())
}

async fn sleep_or_cancel(duration: Duration, cancel: &CancellationToken) {
    tokio::select! {
        () = cancel.cancelled() => {},
        () = tokio::time::sleep(duration) => {},
    }
}
