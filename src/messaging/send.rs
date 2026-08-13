//! Text message construction and sending.

use std::sync::Arc;

use crate::api::client::HttpApiClient;
use crate::error::Result;
use crate::messaging::inbound::SendResult;
use crate::types::{
    BaseInfo, MessageItem, MessageItemType, MessageState, MessageType, SendMessageRequest,
    TextItem, WeixinMessage,
};
use crate::util::random::generate_id;

/// Generate a client ID for outbound messages.
pub fn generate_client_id() -> String {
    generate_id("weixin-agent")
}

/// Build a `SendMessageRequest` for a text message.
///
/// `run_id` groups this message with others of the same logical outbound run;
/// pass `None` when there is no run context.
pub fn build_text_message(
    to: &str,
    text: &str,
    context_token: Option<&str>,
    run_id: Option<&str>,
    base_info: BaseInfo,
) -> SendMessageRequest {
    let item_list = if text.is_empty() {
        None
    } else {
        Some(vec![MessageItem {
            item_type: Some(MessageItemType::Text),
            text_item: Some(TextItem {
                text: Some(text.to_owned()),
            }),
            ..Default::default()
        }])
    };

    build_request(to, item_list, context_token, run_id, base_info)
}

/// Build a `SendMessageRequest` carrying exactly one message item.
///
/// The reference implementation sends each progress item as its own request so
/// that `item_list` always holds exactly one entry; this SDK keeps that
/// convention for behavioural equivalence.
pub(crate) fn build_item_message(
    to: &str,
    item: MessageItem,
    context_token: Option<&str>,
    run_id: Option<&str>,
    base_info: BaseInfo,
) -> SendMessageRequest {
    build_request(to, Some(vec![item]), context_token, run_id, base_info)
}

/// Assemble an outbound request envelope — the one place where the bot-side
/// message fields are populated.
fn build_request(
    to: &str,
    item_list: Option<Vec<MessageItem>>,
    context_token: Option<&str>,
    run_id: Option<&str>,
    base_info: BaseInfo,
) -> SendMessageRequest {
    SendMessageRequest {
        msg: WeixinMessage {
            from_user_id: Some(String::new()),
            to_user_id: Some(to.to_owned()),
            client_id: Some(generate_client_id()),
            message_type: Some(MessageType::Bot),
            message_state: Some(MessageState::Finish),
            item_list,
            context_token: context_token.map(String::from),
            run_id: run_id.map(String::from),
            ..Default::default()
        },
        base_info,
    }
}

/// Send a text message and return the client ID.
pub(crate) async fn send_text(
    api: &Arc<HttpApiClient>,
    to: &str,
    text: &str,
    context_token: Option<&str>,
    run_id: Option<&str>,
    filter_markdown: bool,
    base_info: BaseInfo,
) -> Result<SendResult> {
    let text = if filter_markdown {
        crate::messaging::markdown_filter::filter_markdown(text)
    } else {
        text.to_owned()
    };
    let req = build_text_message(to, &text, context_token, run_id, base_info);
    let message_id = req.msg.client_id.clone().unwrap_or_default();
    api.send_message(&req).await?;
    Ok(SendResult { message_id })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messaging::outbound_run::{build_tool_call_result_item, build_tool_call_start_item};
    use crate::types::{ToolCallStatus, build_base_info};

    #[test]
    fn build_text_message_structure() {
        let req = build_text_message("user123", "hi", None, None, build_base_info());
        let msg = &req.msg;
        assert_eq!(msg.to_user_id.as_deref(), Some("user123"));
        assert_eq!(msg.message_type, Some(MessageType::Bot));
        assert_eq!(msg.message_state, Some(MessageState::Finish));
        let items = msg.item_list.as_ref().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].item_type, Some(MessageItemType::Text));
        assert_eq!(
            items[0].text_item.as_ref().unwrap().text.as_deref(),
            Some("hi")
        );
    }

    #[test]
    fn build_text_message_empty_text() {
        let req = build_text_message("user123", "", None, None, build_base_info());
        assert!(req.msg.item_list.is_none());
    }

    #[test]
    fn build_text_message_with_context_token() {
        let req = build_text_message("u", "t", Some("ctx_tok"), None, build_base_info());
        assert_eq!(req.msg.context_token.as_deref(), Some("ctx_tok"));
    }

    #[test]
    fn generate_client_id_format() {
        let id = generate_client_id();
        assert!(id.starts_with("weixin-agent:"));
    }

    #[test]
    fn build_text_message_carries_run_id() {
        let req = build_text_message("u", "t", None, Some("run-abc"), build_base_info());
        assert_eq!(req.msg.run_id.as_deref(), Some("run-abc"));
    }

    #[test]
    fn build_text_message_without_run_id_omits_field() {
        let req = build_text_message("u", "t", None, None, build_base_info());
        assert!(req.msg.run_id.is_none());
        let json = serde_json::to_string(&req).unwrap();
        assert!(!json.contains("run_id"));
    }

    #[test]
    fn build_item_message_has_exactly_one_item() {
        let item = build_tool_call_start_item("bash", None);
        let req = build_item_message("u", item, None, Some("r1"), build_base_info());
        assert_eq!(req.msg.item_list.as_ref().unwrap().len(), 1);
        assert_eq!(req.msg.run_id.as_deref(), Some("r1"));
        assert_eq!(req.msg.message_type, Some(MessageType::Bot));
        assert_eq!(req.msg.message_state, Some(MessageState::Finish));
    }

    #[test]
    fn tool_call_start_item_shape() {
        let item = build_tool_call_start_item("bash", Some("call-1"));
        assert_eq!(item.item_type, Some(MessageItemType::ToolCallStart));
        assert_eq!(item.is_completed, Some(false));
        assert!(item.create_time_ms.unwrap() > 0);
        let payload = item.tool_call_start_item.as_ref().unwrap();
        assert_eq!(payload.tool_name.as_deref(), Some("bash"));
        assert_eq!(payload.tool_call_id.as_deref(), Some("call-1"));
    }

    #[test]
    fn tool_call_result_item_shape() {
        let item = build_tool_call_result_item("bash", Some("call-1"), ToolCallStatus::Failed);
        assert_eq!(item.item_type, Some(MessageItemType::ToolCallResult));
        assert_eq!(item.is_completed, Some(true));
        assert!(item.create_time_ms.unwrap() > 0);
        let payload = item.tool_call_result_item.as_ref().unwrap();
        assert_eq!(payload.tool_name.as_deref(), Some("bash"));
        assert_eq!(payload.status.as_deref(), Some("failed"));
    }

    #[test]
    fn tool_call_item_omits_tool_call_id_when_none() {
        let item = build_tool_call_start_item("bash", None);
        assert!(
            item.tool_call_start_item
                .as_ref()
                .unwrap()
                .tool_call_id
                .is_none()
        );
        let json = serde_json::to_string(&item).unwrap();
        assert!(!json.contains("tool_call_id"));
    }

    #[test]
    fn tool_call_items_are_not_markdown_filtered() {
        // Tool names are identifiers, not display prose — they must go out verbatim.
        let item = build_tool_call_start_item("**bash**", None);
        assert_eq!(
            item.tool_call_start_item
                .as_ref()
                .unwrap()
                .tool_name
                .as_deref(),
            Some("**bash**")
        );
        let req = build_item_message("u", item, None, None, build_base_info());
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("**bash**"));
    }
}
