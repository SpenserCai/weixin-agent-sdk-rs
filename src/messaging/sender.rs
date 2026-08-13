//! The single outbound assembly path.
//!
//! Every outbound message — replies from a [`crate::MessageContext`], direct sends
//! from a [`crate::WeixinClient`], and tool-call progress from an
//! [`OutboundRun`] — is assembled here, so protocol fields such as `run_id` are
//! threaded through exactly one place.
//!
//! Request construction itself lives in [`crate::messaging::send`]; this module
//! only decides *what* to send and applies configured policy (the markdown
//! filter). It never builds requests inline.

use std::path::Path;
use std::sync::Arc;

use crate::api::client::HttpApiClient;
use crate::api::config_cache::ConfigCache;
use crate::error::Result;
use crate::messaging::inbound::SendResult;
use crate::messaging::outbound_run::OutboundRun;
use crate::types::MessageItem;

/// Outbound sender shared by the client, message contexts, and outbound runs.
pub struct MessageSender {
    pub(crate) api: Arc<HttpApiClient>,
    pub(crate) cdn_base_url: String,
    pub(crate) config_cache: Arc<ConfigCache>,
    pub(crate) markdown_filter_enabled: bool,
}

impl MessageSender {
    /// Send text with an optional run ID. Applies the markdown filter per config.
    pub(crate) async fn send_text(
        &self,
        to: &str,
        text: &str,
        context_token: Option<&str>,
        run_id: Option<&str>,
    ) -> Result<SendResult> {
        crate::messaging::send::send_text(
            &self.api,
            to,
            text,
            context_token,
            run_id,
            self.markdown_filter_enabled,
            self.api.base_info(),
        )
        .await
    }

    /// Upload and send a media file with an optional run ID.
    pub(crate) async fn send_media(
        &self,
        to: &str,
        file_path: &Path,
        context_token: Option<&str>,
        run_id: Option<&str>,
    ) -> Result<SendResult> {
        crate::messaging::send_media::send_media_file(
            &self.api,
            &self.cdn_base_url,
            to,
            file_path,
            "",
            context_token,
            run_id,
            self.api.base_info(),
        )
        .await
    }

    /// Send exactly one pre-built message item with an optional run ID.
    pub(crate) async fn send_item(
        &self,
        to: &str,
        item: MessageItem,
        context_token: Option<&str>,
        run_id: Option<&str>,
    ) -> Result<SendResult> {
        let req = crate::messaging::send::build_item_message(
            to,
            item,
            context_token,
            run_id,
            self.api.base_info(),
        );
        let message_id = req.msg.client_id.clone().unwrap_or_default();
        self.api.send_message(&req).await?;
        Ok(SendResult { message_id })
    }

    /// Start an outbound run with an auto-generated run ID.
    pub(crate) fn run(self: &Arc<Self>, to: &str, context_token: Option<&str>) -> OutboundRun {
        OutboundRun::new(Arc::clone(self), to, context_token)
    }
}
