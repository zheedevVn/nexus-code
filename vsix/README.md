# vsix — NexusCode Agent

File `.vsix` mẫu mang contribution agent-chat cho VisualEditIDE.

## Nội dung

- `package.json` — contribution mới `contributes.nexuscode.agentChat`:
  provider `customPost` trỏ về proxy NexusCode, **không có `{{api_key}}`** trong
  bodyTemplate/endpoint/header nên `requiresAPIKey = false`, cổng chặn key tự
  biến mất. Contribution KHÔNG chứa header `Authorization` tĩnh — token lấy
  từ đăng nhập Google trong app lúc chạy (xem dưới).
- `themes/nexuscode-dark.json` — theme tối kèm theo.
- `snippets/swift.json` — snippets Swift kèm theo.

## Đăng nhập Google (login-only, không API key)

1. Cài extension này vào VisualEditIDE → patch ở `../app-patch/` đăng ký
   provider **NexusCode Agent** từ contribution.
2. Mở **Cấu hình AI** → chọn **NexusCode Agent** → bấm
   **Đăng nhập bằng Google**. App mở trình duyệt (ASWebAuthenticationSession +
   PKCE), redirect về scheme `nexusking:/oauth/google`.
3. Token (access + refresh) lưu vào Keychain của app. Mỗi lần chat, app tự
   làm mới token khi hết hạn rồi gắn `Authorization: Bearer <access_token>`
   và `x-goog-user-project: <số project>` vào request. Đăng xuất xoá token.
4. Quota tính theo tài khoản Google của người dùng (free tier), không theo
   API key.

Contribution mang hai định danh public (không phải secret) để app dùng:

- `oauthClientId` — Google OAuth client ID (loại iOS), dạng
  `...apps.googleusercontent.com`.
- `googleProjectNumber` — số project Google Cloud, dạng số.

Hiện cả hai còn là placeholder (`YOUR_GOOGLE_OAUTH_CLIENT_ID...` /
`YOUR_GOOGLE_CLOUD_PROJECT_NUMBER`). Khi Chủ tịch tạo xong OAuth client
xem README repo (mục "Đăng nhập Google — việc chỉ Chủ tịch làm được"),
điền giá trị thật vào đây, tăng version vsix rồi đóng gói lại.
Endpoint proxy thật: `https://nexuscode.mmovn.space/v1/chat/completions`
(proxy đã deploy trên hosting 26/11/2026, app Node 20.18.3 đang chạy).

## Đóng gói

```bash
npx @vscode/vsce package
# -> nexuscode-agent-0.2.0.vsix

Chưa đóng gói lại vội: OAuth client ID và project number vẫn là
placeholder — đóng gói sau khi Chủ tịch tạo xong OAuth client trên
Google Cloud Console và điền giá trị thật.

## Lưu ý

Bản thân vsix **không chạy được code** trên VisualEditIDE hiện tại — bộ cài của
app chỉ đọc `contributes.themes` và `contributes.snippets`. Phải áp patch ở
`../app-patch/` thì contribution `nexuscode.agentChat` mới được đăng ký thành
provider chat trong app.
