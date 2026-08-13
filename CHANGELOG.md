# Changelog

本文件记录本 crate 的显著变更，格式参照 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，版本号遵循 [语义化版本](https://semver.org/lang/zh-CN/)（`0.x` 的次版本位即破坏性位）。

## [0.3.0] - 2026-08-13

协议参照从 `@tencent-weixin/openclaw-weixin` v2.4.3 对齐到 v2.4.6。

### 新增

- **工具调用进度消息**（item type 11 / 12）：`OutboundRun::tool_call_start()` / `tool_call_result()`，配套 `ToolCallStatus`、`ToolCallStartItem`、`ToolCallResultItem`。

  > 真机实测（2026-08-13，生产 API）：进度消息被服务端接受（HTTP 200、空响应体、**不分配 `message_id`**，而含文本的消息一律返回 `message_id`），但**微信客户端不渲染**。已穷举五种 wire 变体与 `getConfig` 能力字段，均不改变结果 —— 判断为 iLink 侧**预留、尚未启用**的能力。本 SDK 的价值在于：iLink 启用时无需再改协议层。
- **`run_id` 出站关联**：新增 `OutboundRun` 句柄，经 `MessageContext::run()` 或 `WeixinClient::run()` 获取；同一句柄发出的所有消息携带同一个 `run_id`，可用 `with_run_id()` 覆盖为调用方自己的运行标识。真机未观察到 `run_id` 影响客户端呈现，应视为服务端侧关联字段。
- **`MessageHandler::on_token_stale()`**：服务端报告 bot token 失效（`-14`）时回调，携带 `TokenStaleInfo { errcode, pause_duration }`。默认实现为空。
- **`STALE_TOKEN_ERRCODE`**：`-14` 的正名常量。
- **`NetErrorKind`**：传输失败分类（DNS / TCP / TLS / Timeout / Unknown），仅用于日志诊断，不参与任何控制流。
- **`RefMessageInfo::referenced_msg_id`**：引用消息的 item 级 id（此前在入站投影中被丢弃）。

### 修复

- **未知协议枚举值不再使整批消息解析失败**。此前 `getUpdates` 响应中出现一个未知 item 类型（如新增的 11 / 12）会让**整批**消息反序列化失败，且 `get_updates_buf` 不推进，导致长轮询可能卡在同一批消息上。现在未知值保留在 `Unknown(i32)` 变体中。
- **`sendMessage` 不再静默丢消息**。此前 HTTP 200 携带 `ret: -14` 之类的业务失败会被当作发送成功。

  > 校验口径由真机实测确定：成功时该端点**不返回 `ret`** —— 文本/媒体返回 `{"message_id":<i64>}`，工具进度返回**空体**。因此「缺 `ret`」与「空体」都判为成功，只有显式非零 `ret` 才报错。空体也是参考实现（`JSON.parse("")` 抛错）与本 SDK 的行为差异所在。
  >
  > 该校验已在生产环境捕获到真实失败：**突发发送**（实测 34 秒内 18 条）会触发服务端反刷限制，返回 `{"ret":-2,"errmsg":"prepare failed"}`，此后分钟级内该 bot 的所有发送都失败（与 `context_token` 无关）后自愈。升级前这些消息会被判为发送成功并静默丢失。**请务必处理 `send_text` / `reply_text` 返回的 `Err`**，并避免一轮回复里无节制连发。

### 变更

- `CHANNEL_VERSION` → `"2.4.6"`（同时改变 `iLink-App-ClientVersion` 头的编码值）。
- **行为变更**：`send_text` / `send_media` / `reply_*` 在服务端返回非零 `ret` 时改为返回 `Err(Error::Api { .. })`，此前静默成功。这是缺陷修复，但对依赖「发送总是成功」的调用方是可见的行为变化。
- 传输层失败日志不再输出错误原文（它可能携带未脱敏的 URL query），改为输出脱敏 URL + 失败分类。

### 弃用

- `SESSION_EXPIRED_ERRCODE` → 用 `STALE_TOKEN_ERRCODE`（`-14` 表示 token 失效，不是会话过期）。旧常量保留为别名。
- `Error::SessionExpired` → 用 `Error::TokenStale`。旧变体保留但不再由任何代码路径产生。

### 破坏性变更与迁移

| # | 破坏点 | 迁移动作 |
|---|--------|---------|
| B1 | `MessageItemType` / `MessageState` / `MessageType` / `UploadMediaType` / `TypingStatus` / `MediaType` 加 `#[non_exhaustive]` | 对这些枚举做穷尽 `match` 的代码补 `_ =>` 分支 |
| B2 | `MessageItemType` 新增 `ToolCallStart` / `ToolCallResult` / `Unknown(i32)`；`MessageState` / `MessageType` 新增 `Unknown(i32)` | 同 B1；建议在 `_` 分支记录 `.code()` 便于诊断 |
| B3 | 上述三个入站枚举移除 `#[repr(u8)]` 与 `serde_repr` | 用 `.code()`（返回 `i32`）替代 `MessageItemType::Text as u8` 之类的整数 cast |
| B4 | `MessageItem` 新增 `tool_call_start_item` / `tool_call_result_item` | 用结构体字面量构造时改为 `..Default::default()` |
| B5 | `WeixinMessage` 新增 `run_id` | 同 B4 |
| B6 | `RefMessageInfo` 新增 `referenced_msg_id` 并加 `#[non_exhaustive]` | 改为只读使用；此后增补字段不再破坏 |
| B7 | `ToolCallStartItem` / `ToolCallResultItem` 首次引入即带 `#[non_exhaustive]` | 新类型，用 `Default` + 字段赋值构造 |
| B8 | `messaging::send::build_text_message()` 新增 `run_id: Option<&str>` 参数 | 传 `None` 等价于旧行为 |
| B9 | `MessageSender` 从 `messaging::inbound` 迁至 `messaging::sender` | 无需动作，旧路径保留 `pub use` re-export |

`MessageItem` 与 `WeixinMessage` **刻意不加** `#[non_exhaustive]`：它们是出站消息的构造入口，调用方可能需要手工组装。代价是协议每次扩展这两个结构体都会构成一次 B4/B5 类破坏。

## [0.2.0]

协议对齐 `@tencent-weixin/openclaw-weixin` v2.4.3：出站 Markdown 过滤、连接生命周期通知（notifyStart / notifyStop）、配对码验证登录流程。
