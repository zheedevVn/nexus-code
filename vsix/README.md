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
Endpoint proxy (`https://api.nexuscode.example/v1/chat/completions`) cũng
còn là placeholder — thay bằng URL proxy thật sau khi deploy, cùng một lần.

## Đóng gói

```bash
npx @vscode/vsce package
# -> nexuscode-agent-0.1.0.vsix
```

Chưa đóng gói lại vội: endpoint và OAuth client ID vẫn là placeholder.
Đóng gói sau khi có URL proxy thật + OAuth client ID thật.

## Lưu ý

Bản thân vsix **không chạy được code** trên VisualEditIDE hiện tại — bộ cài của
app chỉ đọc `contributes.themes` và `contributes.snippets`. Phải áp patch ở
`../app-patch/` thì contribution `nexuscode.agentChat` mới được đăng ký thành
provider chat trong app.
