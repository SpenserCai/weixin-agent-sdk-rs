//! Protocol types mirroring the Weixin iLink Bot API.

use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

// ── Protocol constants ──────────────────────────────────────────────

/// iLink-App-Id header value.
pub const ILINK_APP_ID: &str = "bot";
/// Channel version sent in `base_info`.
pub const CHANNEL_VERSION: &str = "2.4.6";
/// Fixed QR code base URL.
pub const QR_CODE_BASE_URL: &str = "https://ilinkai.weixin.qq.com/";
/// Default bot type for QR login.
pub const DEFAULT_ILINK_BOT_TYPE: &str = "3";
/// Error code returned when the bot token is stale / invalid.
///
/// The bot must be re-authenticated (QR login) before polling can resume.
pub const STALE_TOKEN_ERRCODE: i32 = -14;
/// Deprecated alias of [`STALE_TOKEN_ERRCODE`].
#[deprecated(
    since = "0.3.0",
    note = "use STALE_TOKEN_ERRCODE — -14 means the token is stale, not the session"
)]
pub const SESSION_EXPIRED_ERRCODE: i32 = STALE_TOKEN_ERRCODE;
/// Text chunk limit (characters).
pub const TEXT_CHUNK_LIMIT: usize = 4000;

// ── Timing constants (ms) ───────────────────────────────────────────

/// Long-poll timeout.
pub const DEFAULT_LONG_POLL_TIMEOUT_MS: u64 = 35_000;
/// Regular API timeout.
pub const DEFAULT_API_TIMEOUT_MS: u64 = 15_000;
/// Config/typing API timeout.
pub const DEFAULT_CONFIG_TIMEOUT_MS: u64 = 10_000;
/// Poll-loop pause after the server reports a stale token.
pub const SESSION_PAUSE_DURATION_MS: u64 = 3_600_000;
/// Max consecutive poll failures before backoff.
pub const MAX_CONSECUTIVE_FAILURES: u32 = 3;
/// Backoff delay after max failures.
pub const BACKOFF_DELAY_MS: u64 = 30_000;
/// Normal retry delay.
pub const RETRY_DELAY_MS: u64 = 2_000;
/// CDN upload max retries.
pub const UPLOAD_MAX_RETRIES: u32 = 3;
/// Config cache TTL.
pub const CONFIG_CACHE_TTL_MS: u64 = 86_400_000;
/// Max QR refresh count.
pub const MAX_QR_REFRESH_COUNT: u32 = 3;
/// QR poll timeout.
pub const DEFAULT_QR_POLL_TIMEOUT_MS: u64 = 35_000;

// ── Enums ───────────────────────────────────────────────────────────

/// CDN upload media type.
#[derive(Debug, Clone, Copy, Serialize_repr, Deserialize_repr, PartialEq, Eq)]
#[repr(u8)]
#[non_exhaustive]
pub enum UploadMediaType {
    /// Image upload.
    Image = 1,
    /// Video upload.
    Video = 2,
    /// Generic file upload.
    File = 3,
    /// Voice upload.
    Voice = 4,
}

/// Message sender type.
///
/// Unknown wire values are preserved in [`MessageType::Unknown`] instead of
/// failing deserialization — see [`MessageItemType`] for the rationale.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum MessageType {
    /// Unset.
    #[default]
    None,
    /// From a human user.
    User,
    /// From a bot.
    Bot,
    /// Wire value not known to this SDK version.
    Unknown(i32),
}

impl MessageType {
    /// Wire value for this variant.
    pub fn code(self) -> i32 {
        match self {
            Self::None => 0,
            Self::User => 1,
            Self::Bot => 2,
            Self::Unknown(n) => n,
        }
    }

    /// Build from a wire value; unrecognized values map to [`MessageType::Unknown`].
    pub fn from_code(code: i32) -> Self {
        match code {
            0 => Self::None,
            1 => Self::User,
            2 => Self::Bot,
            n => Self::Unknown(n),
        }
    }
}

/// Message item content type.
///
/// Unknown wire values are preserved in [`MessageItemType::Unknown`] instead of
/// failing deserialization — the protocol adds item types over time, and one
/// unrecognized item must not invalidate an entire `getUpdates` batch.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum MessageItemType {
    /// Unset.
    #[default]
    None,
    /// Text content.
    Text,
    /// Image content.
    Image,
    /// Voice content.
    Voice,
    /// File attachment.
    File,
    /// Video content.
    Video,
    /// Tool call started (progress message).
    ToolCallStart,
    /// Tool call finished (progress message).
    ToolCallResult,
    /// Wire value not known to this SDK version.
    Unknown(i32),
}

impl MessageItemType {
    /// Wire value for this variant.
    pub fn code(self) -> i32 {
        match self {
            Self::None => 0,
            Self::Text => 1,
            Self::Image => 2,
            Self::Voice => 3,
            Self::File => 4,
            Self::Video => 5,
            Self::ToolCallStart => 11,
            Self::ToolCallResult => 12,
            Self::Unknown(n) => n,
        }
    }

    /// Build from a wire value; unrecognized values map to [`MessageItemType::Unknown`].
    pub fn from_code(code: i32) -> Self {
        match code {
            0 => Self::None,
            1 => Self::Text,
            2 => Self::Image,
            3 => Self::Voice,
            4 => Self::File,
            5 => Self::Video,
            11 => Self::ToolCallStart,
            12 => Self::ToolCallResult,
            n => Self::Unknown(n),
        }
    }
}

/// Message generation state.
///
/// Unknown wire values are preserved in [`MessageState::Unknown`] instead of
/// failing deserialization — see [`MessageItemType`] for the rationale.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum MessageState {
    /// New / finished.
    #[default]
    New,
    /// Still generating (streaming).
    Generating,
    /// Generation complete.
    Finish,
    /// Wire value not known to this SDK version.
    Unknown(i32),
}

impl MessageState {
    /// Wire value for this variant.
    pub fn code(self) -> i32 {
        match self {
            Self::New => 0,
            Self::Generating => 1,
            Self::Finish => 2,
            Self::Unknown(n) => n,
        }
    }

    /// Build from a wire value; unrecognized values map to [`MessageState::Unknown`].
    pub fn from_code(code: i32) -> Self {
        match code {
            0 => Self::New,
            1 => Self::Generating,
            2 => Self::Finish,
            n => Self::Unknown(n),
        }
    }
}

/// Implement `Serialize`/`Deserialize` as a bare protocol integer, preserving
/// unknown values. Replaces `serde_repr`, which cannot express a data-carrying
/// fallback variant (standards §2.7 exception).
macro_rules! impl_wire_int_serde {
    ($ty:ty) => {
        impl Serialize for $ty {
            fn serialize<S: serde::Serializer>(
                &self,
                serializer: S,
            ) -> std::result::Result<S::Ok, S::Error> {
                serializer.serialize_i32(self.code())
            }
        }

        impl<'de> Deserialize<'de> for $ty {
            fn deserialize<D: serde::Deserializer<'de>>(
                deserializer: D,
            ) -> std::result::Result<Self, D::Error> {
                Ok(Self::from_code(i32::deserialize(deserializer)?))
            }
        }
    };
}

impl_wire_int_serde!(MessageType);
impl_wire_int_serde!(MessageItemType);
impl_wire_int_serde!(MessageState);

/// Typing indicator status.
#[derive(Debug, Clone, Copy, Serialize_repr, Deserialize_repr, PartialEq, Eq)]
#[repr(u8)]
#[non_exhaustive]
pub enum TypingStatus {
    /// Currently typing.
    Typing = 1,
    /// Cancel typing indicator.
    Cancel = 2,
}

/// High-level media type for inbound messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum MediaType {
    /// Image media.
    Image,
    /// Video media.
    Video,
    /// Voice media.
    Voice,
    /// Generic file.
    File,
}

/// Outcome of a tool call, as reported to the peer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ToolCallStatus {
    /// Finished successfully.
    Completed,
    /// Finished with an error.
    Failed,
    /// Blocked (e.g. awaiting authorization).
    Blocked,
    /// Outcome not determined.
    Unknown,
}

impl ToolCallStatus {
    /// Wire representation.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Blocked => "blocked",
            Self::Unknown => "unknown",
        }
    }
}

// ── BaseInfo ────────────────────────────────────────────────────────

/// Metadata attached to every outgoing API request.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BaseInfo {
    /// Channel version string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_version: Option<String>,
    /// Bot agent UA string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bot_agent: Option<String>,
}

/// Build a `BaseInfo` with the current channel version and bot agent.
pub fn build_base_info_with_agent(bot_agent: &str) -> BaseInfo {
    BaseInfo {
        channel_version: Some(CHANNEL_VERSION.to_owned()),
        bot_agent: Some(bot_agent.to_owned()),
    }
}

/// Build a `BaseInfo` with the current channel version (legacy, prefer `build_base_info_with_agent`).
pub fn build_base_info() -> BaseInfo {
    build_base_info_with_agent("weixin-agent-rs")
}

// ── CDN / Media sub-structures ──────────────────────────────────────

/// CDN media reference.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CdnMedia {
    /// Encrypted query parameter for CDN download.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encrypt_query_param: Option<String>,
    /// AES key (base64-encoded).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aes_key: Option<String>,
    /// Encrypt type: 0 = fileid only, 1 = packed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encrypt_type: Option<i32>,
    /// Full download URL from server.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub full_url: Option<String>,
}

/// Text item.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TextItem {
    /// Text content.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

/// Image item.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ImageItem {
    /// Original image CDN reference.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media: Option<CdnMedia>,
    /// Thumbnail CDN reference.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb_media: Option<CdnMedia>,
    /// Raw AES key as hex string (preferred for inbound decryption).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aeskey: Option<String>,
    /// Image URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Mid-size ciphertext bytes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mid_size: Option<i64>,
    /// Thumbnail size.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb_size: Option<i64>,
    /// Thumbnail height.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb_height: Option<i64>,
    /// Thumbnail width.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb_width: Option<i64>,
    /// HD size.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hd_size: Option<i64>,
}

/// Voice item.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VoiceItem {
    /// Voice CDN reference.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media: Option<CdnMedia>,
    /// Encoding type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encode_type: Option<i32>,
    /// Bits per sample.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bits_per_sample: Option<i32>,
    /// Sample rate (Hz).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_rate: Option<i32>,
    /// Duration in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub playtime: Option<i64>,
    /// Speech-to-text result.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

/// File item.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileItem {
    /// File CDN reference.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media: Option<CdnMedia>,
    /// Original file name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_name: Option<String>,
    /// File MD5.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub md5: Option<String>,
    /// Plaintext file size as string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub len: Option<String>,
}

/// Video item.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VideoItem {
    /// Video CDN reference.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media: Option<CdnMedia>,
    /// Video ciphertext size.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_size: Option<i64>,
    /// Play length in seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub play_length: Option<i64>,
    /// Video MD5.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_md5: Option<String>,
    /// Thumbnail CDN reference.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb_media: Option<CdnMedia>,
    /// Thumbnail size.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb_size: Option<i64>,
    /// Thumbnail height.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb_height: Option<i64>,
    /// Thumbnail width.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb_width: Option<i64>,
}

/// Reference (quoted) message.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RefMessage {
    /// Quoted message item.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_item: Option<Box<MessageItem>>,
    /// Summary title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

/// Tool call start payload (item type 11).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ToolCallStartItem {
    /// Tool name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    /// Caller-assigned tool call ID, used to pair start with result.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

/// Tool call result payload (item type 12).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ToolCallResultItem {
    /// Tool name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    /// Tool call ID matching the corresponding start item.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// Normalized status string (see [`ToolCallStatus::as_str`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

/// A single content item within a message.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MessageItem {
    /// Item type.
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub item_type: Option<MessageItemType>,
    /// Creation timestamp (ms).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_time_ms: Option<i64>,
    /// Update timestamp (ms).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_time_ms: Option<i64>,
    /// Whether generation is complete.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_completed: Option<bool>,
    /// Item-level message ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub msg_id: Option<String>,
    /// Referenced (quoted) message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ref_msg: Option<RefMessage>,
    /// Text content.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_item: Option<TextItem>,
    /// Image content.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_item: Option<ImageItem>,
    /// Voice content.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voice_item: Option<VoiceItem>,
    /// File content.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_item: Option<FileItem>,
    /// Video content.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_item: Option<VideoItem>,
    /// Tool call start content.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_start_item: Option<ToolCallStartItem>,
    /// Tool call result content.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_result_item: Option<ToolCallResultItem>,
}

// ── WeixinMessage ───────────────────────────────────────────────────

/// Unified message from `getUpdates`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WeixinMessage {
    /// Sequence number.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seq: Option<i64>,
    /// Server-assigned message ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<i64>,
    /// Sender user ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_user_id: Option<String>,
    /// Recipient user ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_user_id: Option<String>,
    /// Client-generated message ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    /// Creation timestamp (ms).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_time_ms: Option<i64>,
    /// Update timestamp (ms).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_time_ms: Option<i64>,
    /// Deletion timestamp (ms); >0 means recalled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delete_time_ms: Option<i64>,
    /// Session ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Group ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_id: Option<String>,
    /// Sender type (user / bot).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_type: Option<MessageType>,
    /// Generation state.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_state: Option<MessageState>,
    /// Content items.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_list: Option<Vec<MessageItem>>,
    /// Context token for replies.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_token: Option<String>,
    /// Run ID grouping all messages of one logical outbound run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
}

// ── API request / response types ────────────────────────────────────

/// `getUpdates` request body.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetUpdatesRequest {
    /// Full context buf from previous response.
    pub get_updates_buf: String,
    /// Metadata.
    pub base_info: BaseInfo,
}

/// `getUpdates` response body.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GetUpdatesResponse {
    /// Return code (0 = success).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ret: Option<i32>,
    /// Error code.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errcode: Option<i32>,
    /// Error message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errmsg: Option<String>,
    /// Inbound messages.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub msgs: Option<Vec<WeixinMessage>>,
    /// Legacy sync buf (compat).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sync_buf: Option<String>,
    /// New context buf to cache.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub get_updates_buf: Option<String>,
    /// Server-suggested next poll timeout (ms).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub longpolling_timeout_ms: Option<u64>,
}

/// `sendMessage` request body.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendMessageRequest {
    /// The message to send.
    pub msg: WeixinMessage,
    /// Metadata.
    pub base_info: BaseInfo,
}

/// `sendMessage` response body (internal).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct SendMessageResponse {
    /// Return code (0 or absent = success).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ret: Option<i32>,
    /// Error message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errmsg: Option<String>,
}

/// `getUploadUrl` request body.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetUploadUrlRequest {
    /// Random file key (32 hex chars).
    pub filekey: String,
    /// Upload media type.
    pub media_type: UploadMediaType,
    /// Recipient user ID.
    pub to_user_id: String,
    /// Plaintext file size.
    pub rawsize: u64,
    /// Plaintext file MD5 hex.
    pub rawfilemd5: String,
    /// Ciphertext file size.
    pub filesize: u64,
    /// Whether thumbnail is not needed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub no_need_thumb: Option<bool>,
    /// Thumbnail plaintext size.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb_rawsize: Option<u64>,
    /// Thumbnail plaintext MD5 hex.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb_rawfilemd5: Option<String>,
    /// Thumbnail ciphertext size.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb_filesize: Option<u64>,
    /// AES key hex string.
    pub aeskey: String,
    /// Metadata.
    pub base_info: BaseInfo,
}

/// `getUploadUrl` response body.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GetUploadUrlResponse {
    /// Upload encrypted parameter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upload_param: Option<String>,
    /// Thumbnail upload parameter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb_upload_param: Option<String>,
    /// Full upload URL from server.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upload_full_url: Option<String>,
}

/// `getConfig` request body (internal).
#[derive(Debug, Clone, Serialize)]
pub(crate) struct GetConfigRequest {
    /// User ID to get config for.
    pub ilink_user_id: String,
    /// Optional context token.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_token: Option<String>,
    /// Metadata.
    pub base_info: BaseInfo,
}

/// `getConfig` response body.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GetConfigResponse {
    /// Return code.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ret: Option<i32>,
    /// Error message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errmsg: Option<String>,
    /// Typing ticket (base64).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub typing_ticket: Option<String>,
}

/// `sendTyping` request body.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendTypingRequest {
    /// Target user ID.
    pub ilink_user_id: String,
    /// Typing ticket from `getConfig`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub typing_ticket: Option<String>,
    /// Typing status.
    pub status: TypingStatus,
    /// Metadata.
    pub base_info: BaseInfo,
}

// ── QR login types ──────────────────────────────────────────────────

/// QR code response from server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QrCodeResponse {
    /// QR code token string.
    pub qrcode: String,
    /// QR code image URL.
    pub qrcode_img_content: String,
}

/// QR status response from server.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QrStatusResponse {
    /// Current status.
    pub status: String,
    /// Bot token (on confirmed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bot_token: Option<String>,
    /// Bot ID (on confirmed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ilink_bot_id: Option<String>,
    /// Base URL (on confirmed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baseurl: Option<String>,
    /// User ID who scanned.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ilink_user_id: Option<String>,
    /// Redirect host for IDC redirect.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redirect_host: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_type_round_trips_known_values() {
        for code in [0, 1, 2, 3, 4, 5, 11, 12] {
            assert_eq!(MessageItemType::from_code(code).code(), code);
        }
    }

    #[test]
    fn item_type_preserves_unknown_value() {
        let t = MessageItemType::from_code(99);
        assert_eq!(t, MessageItemType::Unknown(99));
        assert_eq!(t.code(), 99);
    }

    #[test]
    fn item_type_serializes_as_wire_int() {
        assert_eq!(
            serde_json::to_string(&MessageItemType::ToolCallStart).unwrap(),
            "11"
        );
        assert_eq!(
            serde_json::to_string(&MessageItemType::ToolCallResult).unwrap(),
            "12"
        );
        assert_eq!(
            serde_json::to_string(&MessageItemType::Unknown(77)).unwrap(),
            "77"
        );
    }

    #[test]
    fn unknown_item_type_does_not_break_batch() {
        // Regression: one unrecognized item must not invalidate the whole getUpdates batch.
        let json = r#"{"ret":0,"msgs":[
            {"message_type":1,"from_user_id":"u1","item_list":[{"type":1,"text_item":{"text":"hi"}}]},
            {"message_type":2,"item_list":[{"type":11,"tool_call_start_item":{"tool_name":"bash"}}]},
            {"message_type":2,"item_list":[{"type":99}]}
        ]}"#;
        let resp: GetUpdatesResponse = serde_json::from_str(json).unwrap();
        let msgs = resp.msgs.unwrap();
        assert_eq!(msgs.len(), 3);
        assert_eq!(
            msgs[1].item_list.as_ref().unwrap()[0].item_type,
            Some(MessageItemType::ToolCallStart)
        );
        assert_eq!(
            msgs[2].item_list.as_ref().unwrap()[0].item_type,
            Some(MessageItemType::Unknown(99))
        );
    }

    #[test]
    fn unknown_message_state_does_not_break_parse() {
        let json = r#"{"ret":0,"msgs":[{"message_type":1,"message_state":3}]}"#;
        let resp: GetUpdatesResponse = serde_json::from_str(json).unwrap();
        assert_eq!(
            resp.msgs.unwrap()[0].message_state,
            Some(MessageState::Unknown(3))
        );
    }

    #[test]
    fn unknown_message_type_does_not_break_parse() {
        let json = r#"{"ret":0,"msgs":[{"message_type":7}]}"#;
        let resp: GetUpdatesResponse = serde_json::from_str(json).unwrap();
        assert_eq!(
            resp.msgs.unwrap()[0].message_type,
            Some(MessageType::Unknown(7))
        );
    }

    #[test]
    fn run_id_deserializes_and_serializes() {
        let msg: WeixinMessage = serde_json::from_str(r#"{"run_id":"abc123"}"#).unwrap();
        assert_eq!(msg.run_id.as_deref(), Some("abc123"));
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""run_id":"abc123""#));
        // Absent run_id must not be serialized.
        let empty = WeixinMessage::default();
        assert!(!serde_json::to_string(&empty).unwrap().contains("run_id"));
    }

    #[test]
    fn tool_call_status_wire_strings() {
        assert_eq!(ToolCallStatus::Completed.as_str(), "completed");
        assert_eq!(ToolCallStatus::Failed.as_str(), "failed");
        assert_eq!(ToolCallStatus::Blocked.as_str(), "blocked");
        assert_eq!(ToolCallStatus::Unknown.as_str(), "unknown");
    }

    #[test]
    fn channel_version_matches_reference() {
        assert_eq!(CHANNEL_VERSION, "2.4.6");
        assert_eq!(STALE_TOKEN_ERRCODE, -14);
    }

    #[test]
    fn send_message_response_parses_error_and_empty_object() {
        let err: SendMessageResponse =
            serde_json::from_str(r#"{"ret":-14,"errmsg":"stale"}"#).unwrap();
        assert_eq!(err.ret, Some(-14));
        assert_eq!(err.errmsg.as_deref(), Some("stale"));
        let ok: SendMessageResponse = serde_json::from_str("{}").unwrap();
        assert!(ok.ret.is_none());
    }

    #[test]
    fn item_type_round_trips_i32_bounds_through_serde() {
        // Must go through serde, not just from_code/code — the risk is at the wire layer.
        for code in [i32::MIN, -1, i32::MAX] {
            let t = MessageItemType::from_code(code);
            let json = serde_json::to_string(&t).unwrap();
            assert_eq!(json, code.to_string());
            assert_eq!(serde_json::from_str::<MessageItemType>(&json).unwrap(), t);
        }
    }

    #[test]
    fn item_type_rejects_out_of_i32_range() {
        // Values beyond i32 must fail rather than being silently truncated.
        assert!(serde_json::from_str::<MessageItemType>("2147483648").is_err());
    }
}
