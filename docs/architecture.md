# 整体架构

```mermaid
graph TB
    subgraph "用户应用层"
        APP[用户 Bot 应用]
        PERSIST[持久化 / 账号管理]
    end

    subgraph "weixin-agent SDK"
        CLIENT[WeixinClient<br/>Builder + 生命周期]

        subgraph "协议模块"
            API[api<br/>iLink HTTP API]
            MONITOR[monitor<br/>长轮询循环]
            MSG[messaging<br/>消息解析/构建]
            QR[qr_login<br/>QR 码 API]
        end

        subgraph "基础设施"
            CDN[cdn<br/>上传/下载/AES-ECB]
            MEDIA[media<br/>MIME 检测]
        end

        subgraph "支撑"
            TYPES[types<br/>协议类型]
            ERROR[error<br/>错误处理]
            UTIL[util<br/>脱敏/ID 生成/网络错误分类]
        end
    end

    subgraph "外部服务"
        WECHAT_API[微信 iLink Bot API<br/>ilinkai.weixin.qq.com]
        WECHAT_CDN[微信 CDN<br/>novac2c.cdn.weixin.qq.com]
    end

    APP --> CLIENT
    APP --> PERSIST
    CLIENT --> MONITOR
    CLIENT --> MSG
    CLIENT --> QR
    MONITOR --> API
    MSG --> API
    MSG --> CDN
    MSG --> MEDIA
    CDN --> API
    API --> WECHAT_API
    CDN --> WECHAT_CDN
```

## 模块职责

| 模块 | 职责 |
|------|------|
| `client` | SDK 入口，Builder 模式，生命周期管理，优雅关闭 |
| `api` | 7 个 iLink Bot HTTP API 端点封装 + Session Guard（长轮询冷却）+ Config Cache |
| `monitor` | 长轮询循环，断线重连，退避策略，sync_buf 回调，Token 失效回调 |
| `messaging` | 入站消息解析，出站消息构建（统一装配入口），出站 run 与工具调用进度，Context Token 内存管理 |
| `cdn` | CDN 文件上传/下载，AES-128-ECB 加解密 |
| `qr_login` | QR 码获取 + 状态轮询 API |
| `media` | MIME 类型检测（46 项映射） |
| `types` | 协议数据类型、常量、枚举（入站枚举保留未知 wire 值） |
| `error` | 统一错误枚举（`#[non_exhaustive]`） |
| `util` | 日志脱敏、随机 ID 生成、网络错误分类 |
