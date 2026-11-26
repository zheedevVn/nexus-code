# Deploy NexusCode proxy 0.3.0

Proxy Rust (axum) — `POST /v1/chat/completions` OpenAI-compatible → Gemini.
LOGIN-ONLY: khong con API key. App dang nhap Google, gui
`Authorization: Bearer <access_token>`; proxy chuyen tiep token do + header
`x-goog-user-project` sang Gemini. Quota tinh theo tai khoan Google cua
tung nguoi dung (free tier). Khong co Bearer thi tra 401 `login required`.

## Buoc 0 — So project Google Cloud (lay sau khi tao OAuth client)

1. Tao Google Cloud project + OAuth client iOS theo huong dan o README repo
   (muc "Dang nhap Google — viec chi Chu tich lam duoc").
2. Lay **so project** (Project number, dang so, vd `123456789012`) trong
   Google Cloud Console → Project settings. Day la dinh danh public,
   **khong phai secret** — duoc phep dat vao env binh thuong.
3. Dat `GCP_PROJECT_NUMBER` vao env cua noi deploy (lenh `fly secrets set` /
   muc Environment cua Render). Neu de trong, app phai tu gui header
   `x-goog-user-project` (app lay tu vsix contribution).

## Phuong an A — Fly.io

- Goi Free ca nhan (mo lai 23/11/2026): $5/thang, can the de kich hoat.
  May shared-cpu-1x 256MB chay lien tuc het ~$2.19/thang → nam trong $5,
  khong ngu, SSE chay tot, vung `sin` (Singapore) gan VN.
- **Chu y muc tran $5:** vuot $5 thi tu chuyen Pay As You Go va tinh ca
  thang tu $0 (khong co gioi han cung). Proxy nay nhe, luu luong chat
  thong thuong khong cham tran.
- Lenhs (chay tren PC, trong thu muc `proxy/`):

```sh
fly auth login
fly launch --ha=false        # chon app nexuscode-proxy, vung sin, khong tao Postgres
fly secrets set GCP_PROJECT_NUMBER=<so-project>
fly deploy
curl https://nexuscode-proxy.fly.dev/health   # -> ok
```

## Phuong an B — Render Free (khong can the)

- Free 750 gio/thang, khong can the. Nhuoc diem: ngu sau 15 phut khong
  co traffic, lan chat dau tien sau do cho ~30-60s de may tinh lai.
- Tao Web Service → chon repo nexus-code → Runtime Docker →
  Dockerfile `./proxy/Dockerfile` → dat env `GCP_PROJECT_NUMBER` →
  Deploy. URL dang `https://nexuscode-proxy.onrender.com`.

## Phuong an C — Hosting cPanel (khuyen dung, xem `proxy-node/README.md`)

Ban Node (`proxy-node/`) cung hanh vi, chay tren hosting cPanel bang
Setup Node.js App, khong can the, khong can tai khoan moi.

## Sau khi co URL that

1. ~~Thay endpoint placeholder trong `vsix/package.json`~~ — ĐÃ XONG
   26/11/2026: endpoint thật `https://nexuscode.mmovn.space/v1/chat/completions`,
   version vsix 0.2.0 (proxy đã deploy, app Node 20.18.3 đang chạy).
2. Tang version vsix 0.1.0 → 0.2.0, dong goi lai `.vsix`.
3. Dong bo GitHub + GitLab, build VisualEditIDE qua GitHub Actions
   (workflow `build-ipa.yml` tu chay khi push len main) de kiem
   chung patch Swift.

## Gioi han da biet

- Sau proxy (Fly/Render), moi request deu den tu IP cua proxy nen
  rate-limit 30 req/phut hieu luc la **toan cuc**, khong phai theo tung
  nguoi dung. Voi 1 nguoi dung thi du.
- Proxy khong xac thuc token — no chi chuyen tiep Bearer cua nguoi dung
  sang Gemini. Token het han/bi thu hoi thi Gemini tra 401/403 → proxy
  tra 502; app tu lam moi token (refresh token) hoac bao nguoi dung
  dang nhap lai.
