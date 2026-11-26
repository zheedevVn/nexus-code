# Patch VisualEditIDE — nhận contribution agent-chat từ vsix (ĐÃ LÀM, 25/11/2026; thêm Google Sign-In 26/11/2026)

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
    "useSSEResponse": true, "defaultModel": "gemini-2.5-flash" },
  "oauthClientId": "<...>.apps.googleusercontent.com",
  "googleProjectNumber": "<so project>" }
```

Từ đó dựng một `AIProviderProfile` (preset `custom_post`, format `.customPost`)
với `customPostConfig` lấy từ contribution, rồi post notification
`.nexuscodeAgentChatRegistered` kèm profile. Hai field mới `oauthClientId` /
`googleProjectNumber` (định danh public, không phải secret) cũng được đọc từ
contribution và ghi vào profile (`nexusOAuthClientID` /
`nexusGoogleProjectNumber`).

**Quy tắc bắt buộc — không có `{{api_key}}`:** header chỉ gửi `Content-Type`;
mọi header phụ trong contribution chứa `{{api_key}}` đều bị bỏ qua. Lý do:
`AIProviderProfile.requiresAPIKey` chỉ trả `true` cho customPost khi
bodyTemplate/endpoint/header đang bật nào chứa `{{api_key}}` — không có chuỗi
đó thì profile tự `requiresAPIKey = false`, cổng chặn key ở
`AIAgentService.sendPrompt` (`if profile.requiresAPIKey && key.isEmpty`) tự
mở, cửa sổ chat không còn hiện "(Cần Key)". Contribution cũng KHÔNG chứa
header `Authorization` tĩnh — token lấy từ đăng nhập Google lúc chạy.

### 2. `AIAgentService` — đăng ký profile từ notification

- `init()` đăng ký observer block-based (theo đúng kiểu `LiveServerService`):
  nhận notification → `Task { @MainActor }` → `registerExternalProfile(_:)`.
- `registerExternalProfile(_:)`: cùng tên + cùng endpoint thì chỉ cập nhật cấu
  hình, không tạo trùng; profile mới đăng ký luôn được đặt làm active (giống
  `addProfile`) để cửa sổ chat mở ra dùng ngay. Khi cập nhật profile đã có,
  OAuth client ID + số project cũng được cập nhật theo contribution mới.
- Token observer được giữ trong `externalAgentChatObserver` và hủy trong
  `deinit`.

### 3. Google Sign-In cho provider NexusCode (LOGIN-ONLY, 26/11/2026)

API key đã bỏ hoàn toàn. Đăng nhập Google thay cho key:

- **`Sources/Services/NexusCodeGoogleAuth.swift` (mới):** OAuth 2.0 + PKCE
  (ASWebAuthenticationSession, redirect `nexusking:/oauth/google` — scheme
  `nexusking` đã khai trong project.yml). Client ID lấy từ profile
  (`nexusOAuthClientID`). Access + refresh token + thời hạn lưu Keychain
  (KeychainVault), email hiển thị giải từ id_token (chỉ để hiển thị).
  `validAccessToken()` tự refresh khi hết hạn (trừ 60s). Đăng xuất xoá hết.
  Scope: `openid email profile` +
  `https://www.googleapis.com/auth/generative-language.peruserquota` —
  quota tính theo tài khoản Google của từng người dùng (free tier).
- **`AIProviderProfile` (`AIProviderModels.swift`):** thêm
  `nexusOAuthClientID`, `nexusGoogleProjectNumber` (Codable, lưu kèm profile)
  và `isNexusCode` (customPost có OAuth client ID).
- **`AIProviderClient`:** `streamChat` / `streamCustomPost` /
  `buildCustomPostURLRequest` nhận thêm `extraHeaders: [String: String]`
  (mặc định `[:]` — các provider khác không đổi). Header phụ ghi đè header
  cùng tên trong config; `Authorization` được che (`Bearer ••••`) trong
  preview, token thật không bao giờ hiện.
- **`AIAgentService`:** `nexusLoginHeaders(for:)` — NexusCode thì lấy access
  token (tự refresh) + `x-goog-user-project` từ profile; provider khác trả
  `[:]`. `runGeneration` và `runTerminalAgentCLI` đều lấy header này TRƯỚC
  khi gửi request; chưa đăng nhập thì báo người dùng đăng nhập thay vì gửi
  request để proxy trả 401. `modelName` hiện "(Chưa đăng nhập)" khi
  NexusCode chưa sign-in. Lỗi 401/403 của NexusCode có thông báo riêng
  (hết phiên → đăng nhập lại).
- **`AIProviderSettingsSheet` (`AIProviderProfileEditor`):** mục "Kết nối"
  của profile NexusCode thay ô API key bằng khu vực đăng nhập Google
  (nút "Đăng nhập bằng Google", hiện email khi đã đăng nhập, nút Đăng xuất,
  báo lỗi đăng nhập). Các provider khác giữ nguyên ô API key.

### 4. Không đổi chữ ký hàm, không sửa call site

`installVSIXExtension(_:into:)` giữ nguyên chữ ký; `LeftExplorerDrawer` (nơi
gọi install) không đụng tới. Toàn bộ nối kết đi qua NotificationCenter.
Các hàm `streamChat`/`streamCustomPost`/`buildCustomPostURLRequest` chỉ thêm
tham số mặc định `extraHeaders` — mọi call site cũ biên dịch như cũ.

## Còn thiếu

- **Deploy proxy** (thư mục `proxy-node/` chạy trên hosting cPanel, hoặc
  `proxy/` trên Fly.io/Render): sau khi deploy, sửa `endpoint` trong
  `vsix/package.json` thành URL thật, điền `oauthClientId` +
  `googleProjectNumber` thật, tăng version vsix rồi đóng gói lại.
- **OAuth client do Chủ tịch tạo** (xem README repo): Google Cloud project +
  bật Generative Language API + consent screen + OAuth client loại iOS
  (redirect `nexusking:/oauth/google`). Không cần thẻ. Client ID + số
  project điền vào `vsix/package.json`.
- **Kiểm chứng biên dịch:** patch 25/11 đã build thành công qua GitHub
  Actions (`build-ipa.yml`, run 25/11 11:40). Patch Google Sign-In 26/11
  chưa được biên dịch kiểm chứng trên VM này (không có Xcode) — chỉ kiểm
  bằng đọc lại diff + cân bằng ngoặc + đối chiếu symbol; lần push lên
  GitHub main sẽ trigger Actions build và là kiểm chứng thật.
- **Chưa test chạy thật:** đăng nhập Google trên iPhone thật (ASWebAuthenticationSession
  + redirect scheme), refresh token, chat qua proxy login-only end-to-end.
