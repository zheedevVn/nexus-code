# Patch VisualEditIDE — nhận contribution agent-chat từ vsix (ĐÃ LÀM, 25/11/2026)

## Vấn đề

`Sources/Services/ExtensionStoreService.swift` khi cài `.vsix` chỉ đọc:

- `contributes.themes`
- `contributes.snippets`

Không có extension host, không chạy code JS của extension. Một vsix chat gắn vào
như hiện tại sẽ không chạy được.

## Patch đã áp (commit: NexusCode agent-chat)

### 1. `ExtensionStoreService.installVSIXExtension` — đọc thêm contribution

Sau mục 1 (Themes) và 2 (Snippets), thêm mục 3: đọc
`contributes.nexuscode.agentChat` từ `package.json` của vsix:

```json
{ "id": "nexuscode-agent", "title": "NexusCode Agent",
  "provider": { "kind": "customPost",
    "endpoint": "https://<proxy>/v1/chat/completions",
    "bodyTemplate": "{\"model\": \"{{model}}\", \"messages\": {{messages}}, \"stream\": {{stream}}}",
    "responsePath": "choices.0.message.content",
    "streamDeltaPath": "choices.0.delta.content",
    "useSSEResponse": true, "defaultModel": "gemini-2.5-flash" } }
```

Từ đó dựng một `AIProviderProfile` (preset `custom_post`, format `.customPost`)
với `customPostConfig` lấy từ contribution, rồi post notification
`.nexuscodeAgentChatRegistered` kèm profile.

**Quy tắc bắt buộc — không có `{{api_key}}`:** header chỉ gửi `Content-Type`;
mọi header phụ trong contribution chứa `{{api_key}}` đều bị bỏ qua. Lý do:
`AIProviderProfile.requiresAPIKey` chỉ trả `true` cho customPost khi
bodyTemplate/endpoint/header đang bật nào chứa `{{api_key}}` — không có chuỗi
đó thì profile tự `requiresAPIKey = false`, cổng chặn key ở
`AIAgentService.sendPrompt` (`if profile.requiresAPIKey && key.isEmpty`) tự
mở, cửa sổ chat không còn hiện "(Cần Key)".

### 2. `AIAgentService` — đăng ký profile từ notification

- `init()` đăng ký observer block-based (theo đúng kiểu `LiveServerService`):
  nhận notification → `Task { @MainActor }` → `registerExternalProfile(_:)`.
- `registerExternalProfile(_:)`: cùng tên + cùng endpoint thì chỉ cập nhật cấu
  hình, không tạo trùng; profile mới đăng ký luôn được đặt làm active (giống
  `addProfile`) để cửa sổ chat mở ra dùng ngay.
- Token observer được giữ trong `externalAgentChatObserver` và hủy trong
  `deinit`.

### 3. Không đổi chữ ký hàm, không sửa call site

`installVSIXExtension(_:into:)` giữ nguyên chữ ký; `LeftExplorerDrawer` (nơi
gọi install) không đụng tới. Toàn bộ nối kết đi qua NotificationCenter.

## Còn thiếu

- **Deploy proxy** (thư mục `proxy/`): cần một nơi chạy được binary Rust
  (VPS / Fly.io / serverless Rust) — hosting cPanel VN2 hiện tại không chạy
  được. Sau khi deploy, sửa `endpoint` trong `vsix/package.json` thành URL
  thật rồi đóng gói lại vsix.
- **Build app** qua farm Mac (Codemagic) — VM này không build được project
  Xcode. **Lưu ý: patch Swift chưa được biên dịch kiểm chứng trên VM này**,
  chỉ kiểm bằng đọc lại diff + cân bằng ngoặc; lần build Codemagic đầu tiên
  sẽ là kiểm chứng thật.
- **Phase 2 — đăng nhập bằng Zhee:** proxy hiện không yêu cầu Bearer
  (chống lạm dụng bằng rate-limit 30 req/phút/IP). Khi app có login Zhee
  (ASWebAuthenticationSession → token lưu Keychain → gửi `Authorization:
  Bearer <token>`), proxy sẽ xác thực token rồi mới gọi Gemini.
