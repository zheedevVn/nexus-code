# NexusCode

App giống VS Code trên iPhone: cửa sổ chat AI agent, cài extension `.vsix`, **đăng nhập tài khoản là dùng được — không cần API key**.

## Kiến trúc — 3 phần

```
┌────────────────────┐      ┌────────────────────┐      ┌────────────────────┐
│  VisualEditIDE     │      │  .vsix             │      │  Proxy (Rust/Node) │
│  (app iOS, patch)  │◄─────│  nexuscode-agent   │◄─────│  chuyển tiếp token │
│  cửa sổ chat agent │      │  contribution      │      │  Google của user   │
└────────────────────┘      └────────────────────┘      └────────────────────┘
```

1. **Proxy (`proxy/`, Rust + `proxy-node/`, Node)** — LOGIN-ONLY, không còn
   API key. App đăng nhập Google, gửi `Authorization: Bearer <access_token>`;
   proxy chuyển tiếp token đó + header `x-goog-user-project` sang Gemini theo
   chuẩn OpenAI-compatible `POST /v1/chat/completions` (SSE streaming).
   Quota tính theo tài khoản Google của từng người dùng (free tier). Không
   Bearer thì trả 401 `login required`. Proxy có rate-limit chống lạm dụng.
2. **Patch app (`app-patch/`)** — dạy `ExtensionStoreService` của VisualEditIDE
   nhận thêm contribution kiểu agent-chat trong `package.json` của vsix, đăng ký
   thành một Custom POST provider trỏ về proxy, không có `{{api_key}}` nên cổng
   chặn key tự biến mất; thêm Google Sign-In (ASWebAuthenticationSession +
   PKCE, token Keychain, tự refresh) cho provider NexusCode.
3. **File `.vsix` (`vsix/`)** — mang contribution đó (kèm theme + snippets +
   OAuth client ID + số project Google Cloud), cài vào là hiện cửa sổ chat agent.

## Vì sao phải làm cả 3 phần

Bộ cài `.vsix` của VisualEditIDE hiện tại **chỉ đọc `contributes.themes` và
`contributes.snippets`** — không có extension host, không chạy code JS của
extension. Nên một file vsix chat gắn vào như hiện tại sẽ không chạy được — không
phải vì thiếu key, mà vì app chưa biết chạy extension. Phải patch app (phần 2)
thì vsix (phần 3) mới có chỗ bám, và phải có proxy (phần 1) thì mới xài được mà
không cần key.

## Trạng thái (26/11/2026)

- [x] Repo + khung 3 phần
- [x] Proxy 0.3.0 login-only (Rust + Node): bỏ hẳn GEMINI_API_KEY, yêu cầu
  Bearer → 401, forward Bearer + `x-goog-user-project` (env server ưu tiên,
  fallback header client), `cargo check` + `cargo test` (2 test) OK, kiểm
  chứng thật với Gemini giả lập (cả hai bản: 401 / forward header / dạng
  OpenAI / SSE + [DONE] / health). Test Bearer giả lên Google thật: cả hai
  bản trả 502, nhưng lỗi là transport (không tới được Google từ VM lúc test)
  chứ không phải lỗi auth của Google — cần kiểm lại đường upstream.
- [x] Patch app: `ExtensionStoreService` + `AIAgentService` (đã đẩy lên
  VisualEditIDE, commit c5241326) — **đã biên dịch thành công qua GitHub
  Actions (`build-ipa.yml`, run 25/11 11:40)**.
- [x] Patch Google Sign-In (26/11): `NexusCodeGoogleAuth` mới (OAuth + PKCE,
  token Keychain, tự refresh), `extraHeaders` xuyên suốt
  `AIProviderClient` → `AIAgentService`, mục đăng nhập Google trong
  `AIProviderSettingsSheet` — **chưa biên dịch kiểm chứng (VM không có
  Xcode); lần push lên GitHub main sẽ trigger Actions build và là kiểm
  chứng thật**.
- [x] `.vsix` mẫu đủ file (package.json + theme + snippets) — thêm
  `oauthClientId` + `googleProjectNumber` (placeholder), endpoint proxy vẫn
  placeholder.
- [ ] Deploy proxy bản Node lên hosting cPanel (subdomain
  nexuscode.mmovn.space, xem `proxy-node/README.md`) rồi sửa `endpoint`
  trong `vsix/package.json` + điền OAuth client ID / số project thật.
- [ ] Đóng gói `.vsix` (`npx @vscode/vsce package`).
- [ ] Test đăng nhập Google + chat end-to-end trên iPhone thật.

## Đăng nhập Google — việc chỉ Chủ tịch làm được

Không còn Gemini API key nữa. Thay vào đó, khi code đã sẵn, Chủ tịch tạo
**Google Cloud OAuth client (miễn phí, không cần thẻ)** bằng tài khoản Google
của mình — Sếp sẽ đưa đúng từng bước lúc đó, tóm tắt:

1. Google Cloud Console → tạo project mới (hoặc dùng project có sẵn) → bật
   **Generative Language API**.
2. APIs & Services → OAuth consent screen → điền tên app, chọn External,
   thêm tài khoản Google của mình làm Test user.
3. Credentials → Create Credentials → **OAuth client ID → iOS** → điền
   Bundle ID của VisualEditIDE, thêm Redirect URI
   `nexusking:/oauth/google` (scheme `nexusking` đã khai trong app).
4. Lấy **Client ID** (dạng `...apps.googleusercontent.com`) + **số project**
   (Project number, dạng số) → điền vào `vsix/package.json`
   (`oauthClientId`, `googleProjectNumber`) và env `GCP_PROJECT_NUMBER`
   trên server proxy. Cả hai đều là định danh public, không phải secret —
   không cần giấu, chỉ là không tự tạo hộ được vì gắn tài khoản Google
   của Chủ tịch.
5. Tạo subdomain nexuscode.mmovn.space + Node.js App trong cPanel, Sếp up
   code proxy-node và kiểm `/health`.

## Giấy phép

MIT — xem `LICENSE`.
