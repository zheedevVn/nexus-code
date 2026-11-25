# NexusCode

App giống VS Code trên iPhone: cửa sổ chat AI agent, cài extension `.vsix`, **đăng nhập tài khoản là dùng được — không cần API key**.

## Kiến trúc — 3 phần

```
┌────────────────────┐      ┌────────────────────┐      ┌────────────────────┐
│  VisualEditIDE     │      │  .vsix             │      │  Proxy (Rust)      │
│  (app iOS, patch)  │◄─────│  nexuscode-agent   │◄─────│  giữ Gemini key    │
│  cửa sổ chat agent │      │  contribution      │      │  phía server       │
└────────────────────┘      └────────────────────┘      └────────────────────┘
```

1. **Proxy (`proxy/`, Rust)** — server giữ Gemini API key phía server. App chỉ cần
   đăng nhập tài khoản (Đăng nhập bằng Zhee — OAuth) rồi gọi proxy theo chuẩn
   OpenAI-compatible `POST /v1/chat/completions` (SSE streaming). Không key nào
   nằm trên máy người dùng. Proxy có rate-limit chống lạm dụng.
2. **Patch app (`app-patch/`)** — dạy `ExtensionStoreService` của VisualEditIDE
   nhận thêm contribution kiểu agent-chat trong `package.json` của vsix, đăng ký
   thành một Custom POST provider trỏ về proxy, không có `{{api_key}}` nên cổng
   chặn key tự biến mất.
3. **File `.vsix` (`vsix/`)** — mang contribution đó (kèm theme + snippets), cài
   vào là hiện cửa sổ chat agent.

## Vì sao phải làm cả 3 phần

Bộ cài `.vsix` của VisualEditIDE hiện tại **chỉ đọc `contributes.themes` và
`contributes.snippets`** — không có extension host, không chạy code JS của
extension. Nên một file vsix chat gắn vào như hiện tại sẽ không chạy được — không
phải vì thiếu key, mà vì app chưa biết chạy extension. Phải patch app (phần 2)
thì vsix (phần 3) mới có chỗ bám, và phải có proxy (phần 1) thì mới xài được mà
không cần key.

## Trạng thái (25/11/2026)

- [x] Repo + khung 3 phần
- [x] Proxy 0.2.0: nối Gemini thật (generateContent + streamGenerateContent SSE),
  rate-limit 30 req/phút/IP, `cargo check` + chạy thử OK (`/health` → ok)
- [x] Patch app: `ExtensionStoreService` + `AIAgentService` (đã đẩy lên
  VisualEditIDE) — **chưa biên dịch kiểm chứng, lần build Codemagic đầu tiên
  mới là kiểm chứng thật**
- [x] `.vsix` mẫu đủ file (package.json + theme + snippets)
- [ ] Deploy proxy (cần nơi chạy được binary Rust — hosting cPanel hiện tại
  không chạy được) rồi sửa `endpoint` trong `vsix/package.json`
- [ ] Đóng gói `.vsix` (`npx @vscode/vsce package`)
- [ ] Xác thực token Zhee (phase 2) + build app qua farm Mac (Codemagic)

## Việc chỉ Chủ tịch làm được

- Tạo Gemini API key miễn phí tại aistudio.google.com → đặt vào env của server
  (không gửi key qua chat).
- Build app qua Codemagic sau khi patch xong.

## Giấy phép

MIT — xem `LICENSE`.
