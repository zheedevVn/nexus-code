# NexusCode proxy — bản Node (chạy trên hosting cPanel)

Bản Node của proxy Rust (`../proxy/`): cùng hành vi — `POST /v1/chat/completions`
OpenAI-compatible → Gemini (`:generateContent` + `:streamGenerateContent?alt=sse`).
LOGIN-ONLY: không còn API key. App đăng nhập Google, gửi
`Authorization: Bearer <access_token>`; proxy chuyển tiếp token đó + header
`x-goog-user-project` sang Gemini. Quota tính theo tài khoản Google của
từng người dùng (free tier). Không có Bearer thì trả 401 `login required`.
**Không phụ thuộc ngoài** (chỉ dùng `http`/`https` có sẵn của Node),
chạy được từ **Node 14 trở lên**, không cần build.

## Chạy thử ở local

```sh
node server.js            # PORT mac dinh 8080
curl localhost:8080/health   # -> ok
```

Gọi chat không kèm Bearer thì trả 401 (đúng thiết kế).

## Deploy lên hosting cPanel (VNHost) — subdomain nexuscode.mmovn.space

Làm trên PC (cPanel chỉ làm được từ máy Chủ tịch):

1. **Lấy số project Google Cloud:** tạo Google Cloud project + OAuth client
   iOS theo hướng dẫn ở README repo (mục "Đăng nhập Google — việc chỉ
   Chủ tịch làm được"). Lấy **số project** (Project number, dạng số) trong
   Google Cloud Console → Project settings. Đây là định danh public,
   **không phải secret**.
2. **cPanel → Subdomains:** tạo subdomain `nexuscode.mmovn.space`
   (document root mặc định `/home/mmovn1/nexuscode.mmovn.space`).
3. **cPanel → Setup Node.js App → Create Application:**
   - Node.js version: chọn bản cao nhất trong danh sách (khuyến nghị 20;
     code chạy được từ Node 14 nên bản nào cũng được).
   - Application mode: Production.
   - Application root: `/home/mmovn1/nexuscode.mmovn.space`
   - Application URL: `nexuscode.mmovn.space`.
   - Application startup file: `server.js`.
   - Bấm Create, rồi vào mục **Environment variables** thêm
     `GCP_PROJECT_NUMBER` = số project ở bước 1. Bấm Save rồi **Restart** app.
4. **Đưa code lên:** Sếp up file qua file manager vào đúng Application root
   (`server.js`, `package.json`, `.env.example` — không cần `node_modules`,
   không cần `npm install` vì không có phụ thuộc).
5. **Kiểm tra sống:** mở `https://nexuscode.mmovn.space/health` → phải ra `ok`.
   Gọi thử chat không kèm Bearer → 401 `login required`.

## Sau khi có URL thật

Endpoint cho app/VSIX:

```
https://nexuscode.mmovn.space/v1/chat/completions
```

Thay URL này vào `vsix/package.json` — ĐÃ XONG 26/11/2026
(endpoint thật `https://nexuscode.mmovn.space/v1/chat/completions`,
version vsix 0.2.0). Còn lại: điền OAuth client ID + project number
thật sau khi Chủ tịch tạo OAuth client, rồi đóng gói lại `.vsix`,
đồng bộ GitHub + GitLab.

## Lưu ý

- cPanel chạy app qua Passenger + Apache proxy. SSE (`stream: true`) thường
  chạy được; nếu thấy stream bị ngắt/chậm thì app vẫn dùng được ở chế độ
  không stream (bản Rust trên Fly.io không có vấn đề này).
- Log lỗi của app xem ở cPanel → Setup Node.js App → (app) → log, hoặc
  file `stderr.log` trong Application root.
- Proxy không xác thực token — nó chỉ chuyển tiếp Bearer của người dùng
  sang Gemini. Token hết hạn/bị thu hồi thì Gemini trả 401/403 → proxy
  trả 502; app tự làm mới token (refresh token) hoặc báo người dùng
  đăng nhập lại.
- Bản Rust (`../proxy/`, Dockerfile + fly.toml + render.yaml) vẫn giữ nguyên
  cho ai muốn deploy Fly.io/Render sau này.
