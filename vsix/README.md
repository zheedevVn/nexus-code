# vsix — NexusCode Agent

File `.vsix` mẫu mang contribution agent-chat cho VisualEditIDE.

## Nội dung

- `package.json` — contribution mới `contributes.nexuscode.agentChat`:
  provider `customPost` trỏ về proxy NexusCode, **không có `{{api_key}}`** trong
  bodyTemplate/endpoint/header nên `requiresAPIKey = false`, cổng chặn key tự
  biến mất.
- `themes/nexuscode-dark.json` — theme tối kèm theo (TODO).
- `snippets/swift.json` — snippets Swift kèm theo (TODO).

## Đóng gói

```bash
npx @vscode/vsce package
# -> nexuscode-agent-0.1.0.vsix
```

## Lưu ý

Bản thân vsix **không chạy được code** trên VisualEditIDE hiện tại — bộ cài của
app chỉ đọc `contributes.themes` và `contributes.snippets`. Phải áp patch ở
`../app-patch/` thì contribution `nexuscode.agentChat` mới được đăng ký thành
provider chat trong app.
