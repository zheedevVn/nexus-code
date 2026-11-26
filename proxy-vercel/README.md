# NexusCode proxy — Vercel

Bản serverless của proxy Node (`proxy-node/server.js`), chạy trên Vercel vì
hosting VNHost chặn kết nối outbound từ app Node Passenger (chat thật luôn
502, đã kiểm chứng 26/11/2026 với cả Google lẫn httpbin).

## Deploy

- Import repo GitHub `zheedevVn/nexus-code` vào Vercel, **Root Directory:
  `proxy-vercel`**, Project name: `nexuscode-proxy`.
- Env var (Production): `GCP_PROJECT_NUMBER=868862055400`.
- KHÔNG đặt `GEMINI_BASE` (mặc định gọi `generativelanguage.googleapis.com`).
- URL sau deploy: `https://nexuscode-proxy.vercel.app`
  - `GET /health` → `ok`
  - `POST /v1/chat/completions` (Bearer token Google của user) → Gemini.

## Endpoint trong app

Preset NexusCode trong VisualEditIDE trỏ tới
`https://nexuscode-proxy.vercel.app/v1/chat/completions`
(thay cho `https://nexuscode.mmovn.space/v1/chat/completions`).

## Nguồn logic

`api/chat.js` = `proxy-node/server.js` bỏ phần `http.createServer` +
`listen`, export handler Vercel thay thế. Mọi hành vi (401 khi thiếu Bearer,
rate-limit 30 req/phút/IP, normalize contents, SSE) giữ nguyên.
