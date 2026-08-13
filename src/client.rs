//! SDK entry point: [`WeixinClient`] and its builder.

use std::path::Path;
use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use crate::api::client::HttpApiClient;
use crate::api::config_cache::ConfigCache;
use crate::api::session_guard::SessionGuard;
use crate::config::WeixinConfig;
use crate::error::{Error, Result};
use crate::messaging::inbound::{ContextTokenStore, SendResult};
use crate::messaging::outbound_run::OutboundRun;
use crate::messaging::sender::MessageSender;
use crate::monitor::poll_loop::MessageHandler;
use crate::qr_login::login::QrLoginApi;

/// The main SDK client.
pub struct WeixinClient {
    config: Arc<WeixinConfig>,
    handler: Arc<dyn MessageHandler>,
    api: Arc<HttpApiClient>,
    sender: Arc<MessageSender>,
    session_guard: Arc<SessionGuard>,
    context_tokens: Arc<ContextTokenStore>,
    cancel: CancellationToken,
}

/// Builder for [`WeixinClient`].
#[must_use]
pub struct WeixinClientBuilder {
    config: WeixinConfig,
    handler: Option<Arc<dyn MessageHandler>>,
    cancel: CancellationToken,
}

impl WeixinClient {
    /// Create a new builder.
    pub fn builder(config: WeixinConfig) -> WeixinClientBuilder {
        WeixinClientBuilder {
            config,
            handler: None,
            cancel: CancellationToken::new(),
        }
    }

    /// Start the long-poll monitor loop. Blocks until shutdown.
    ///
    /// `initial_sync_buf` should be loaded from your persistence layer (or `None` for fresh start).
    pub async fn start(&self, initial_sync_buf: Option<String>) -> Result<()> {
        if let Err(e) = self.api.notify_start().await {
            // Best-effort call; `post_raw` already logged the classified transport
            // failure. The error text is not repeated here because it can carry an
            // un-redacted URL (standards §1.3).
            tracing::warn!(
                kind = crate::util::net_error::classify(&e).as_str(),
                "notify_start failed"
            );
        }

        crate::monitor::poll_loop::run_monitor(
            Arc::clone(&self.api),
            Arc::clone(&self.sender),
            Arc::clone(&self.handler),
            Arc::clone(&self.session_guard),
            Arc::clone(&self.context_tokens),
            initial_sync_buf,
            self.config.long_poll_timeout,
            self.cancel.clone(),
        )
        .await
    }

    /// Gracefully shut down the monitor loop.
    pub fn shutdown(&self) {
        self.cancel.cancel();
    }

    /// Send a text message to a user.
    pub async fn send_text(
        &self,
        to: &str,
        text: &str,
        context_token: Option<&str>,
    ) -> Result<SendResult> {
        self.sender.send_text(to, text, context_token, None).await
    }

    /// Send a media file to a user.
    pub async fn send_media(
        &self,
        to: &str,
        file_path: &Path,
        context_token: Option<&str>,
    ) -> Result<SendResult> {
        self.sender
            .send_media(to, file_path, context_token, None)
            .await
    }

    /// Start an outbound run addressed to `to`.
    ///
    /// Every message sent through the returned handle carries the same `run_id`,
    /// which lets the peer group them as one logical run.
    pub fn run(&self, to: &str, context_token: Option<&str>) -> OutboundRun {
        self.sender.run(to, context_token)
    }

    /// Get a QR login API handle.
    pub fn qr_login(&self) -> QrLoginApi<'_> {
        QrLoginApi::new(&self.api)
    }

    /// Access the context token store (for export/import).
    pub fn context_tokens(&self) -> &ContextTokenStore {
        &self.context_tokens
    }
}

impl WeixinClientBuilder {
    /// Set the message handler.
    pub fn on_message(mut self, handler: impl MessageHandler + 'static) -> Self {
        self.handler = Some(Arc::new(handler));
        self
    }

    /// Optionally set a cancellation token for the monitor loop (for advanced users).
    pub fn with_cancel_token(mut self, cancel: CancellationToken) -> Self {
        self.cancel = cancel;
        self
    }

    /// Build the client.
    pub fn build(self) -> Result<WeixinClient> {
        let handler = self
            .handler
            .ok_or_else(|| Error::Config("message handler is required".into()))?;
        let api = Arc::new(HttpApiClient::new(&self.config));
        let config_cache = Arc::new(ConfigCache::new(Arc::clone(&api)));
        let sender = Arc::new(MessageSender {
            api: Arc::clone(&api),
            cdn_base_url: self.config.cdn_base_url.clone(),
            config_cache,
            markdown_filter_enabled: self.config.markdown_filter_enabled,
        });
        Ok(WeixinClient {
            config: Arc::new(self.config),
            handler,
            api,
            sender,
            session_guard: Arc::new(SessionGuard::new()),
            context_tokens: Arc::new(ContextTokenStore::new()),
            cancel: self.cancel,
        })
    }
}
