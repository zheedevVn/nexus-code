// NexusCode proxy (Node) 0.3.0 — OpenAI-compatible POST /v1/chat/completions -> Gemini.
// Ban Node cua proxy Rust (proxy/): cung hanh vi, khong phu thuoc (chi dung
// http/https co san cua Node). Chay duoc tu Node 14 tro len, hop de chay tren
// hosting cPanel (Setup Node.js App) — khong can build, khong can Rust.
// LOGIN-ONLY: khong con API key. App dang nhap Google, gui
// `Authorization: Bearer <access_token>`; proxy chuyen tiep token do + header
// `x-goog-user-project` sang Gemini. Quota tinh theo tai khoan Google cua
// tung nguoi dung (free tier), khong phai theo key server.
// Chong lam dung bang rate-limit theo IP (30 req/phut).

'use strict';

const http = require('http');
const https = require('https');

const DEFAULT_MODEL = 'gemini-2.5-flash';
const RATE_LIMIT_PER_MIN = 30;
const GEMINI_BASE = process.env.GEMINI_BASE || 'https://generativelanguage.googleapis.com/v1beta';
const PORT = parseInt(process.env.PORT || '8080', 10) || 8080;
const MAX_BODY = 2 * 1024 * 1024; // 2 MB

const projectNumber = (process.env.GCP_PROJECT_NUMBER || '').trim();
if (!projectNumber) {
  console.warn('[warn] GCP_PROJECT_NUMBER chua dat — request se thieu x-goog-user-project (app phai gui header nay)');
}

// ---------- rate limit: ip -> { windowStart, count } ----------
const hits = new Map();
function rateLimited(ip) {
  const now = Date.now();
  let e = hits.get(ip);
  if (!e || now - e.start > 60000) {
    e = { start: now, count: 0 };
    hits.set(ip, e);
  }
  e.count += 1;
  return e.count > RATE_LIMIT_PER_MIN;
}

// ---------- Gemini doi luan phien user/model xen ke, mo dau phai la user.
// Gop cac message cung role lien nhau, bo message model o dau. ----------
function normalizeContents(messages) {
  let system = null;
  const merged = []; // [role, text]
  for (const m of messages) {
    const role = m.role;
    const content = typeof m.content === 'string' ? m.content : '';
    if (role === 'system') {
      system = system === null ? content : system + '\n\n' + content;
    } else if (role === 'assistant') {
      const last = merged[merged.length - 1];
      if (last && last[0] === 'model') {
        last[1] += '\n\n' + content;
      } else {
        merged.push(['model', content]);
      }
    } else {
      const last = merged[merged.length - 1];
      if (last && last[0] === 'user') {
        last[1] += '\n\n' + content;
      } else {
        merged.push(['user', content]);
      }
    }
  }
  while (merged.length > 0 && merged[0][0] === 'model') merged.shift();
  const contents = merged.map(([role, text]) => ({
    role,
    parts: [{ text }],
  }));
  return { contents, system };
}

function geminiText(resp) {
  let text = '';
  let finish = null;
  const cands = resp && resp.candidates;
  if (Array.isArray(cands) && cands.length > 0) {
    const c = cands[0];
    if (c.finishReason) finish = c.finishReason;
    const parts = c.content && c.content.parts;
    if (Array.isArray(parts)) {
      for (const p of parts) {
        if (p.text) text += p.text;
      }
    }
  }
  return { text, finish };
}

function mapFinish(finish) {
  if (!finish) return null;
  return finish === 'STOP' ? 'stop' : String(finish).toLowerCase();
}

function newId() {
  const nanos = process.hrtime.bigint().toString(16);
  return 'chatcmpl-' + nanos + process.pid.toString(16);
}

function sendJson(res, status, obj) {
  const body = JSON.stringify(obj);
  res.writeHead(status, { 'Content-Type': 'application/json' });
  res.end(body);
}

// Goi len Gemini, tra ve { status, text } (text la body tho).
// auth: { token, project } — token Bearer cua nguoi dung, project co the rong.
function callGemini(path, bodyObj, auth) {
  return new Promise((resolve, reject) => {
    const u = new URL(GEMINI_BASE + path);
    const data = JSON.stringify(bodyObj);
    const lib = u.protocol === 'https:' ? https : http;
    const headers = {
      'Content-Type': 'application/json',
      'Content-Length': Buffer.byteLength(data),
      Authorization: 'Bearer ' + auth.token,
    };
    if (auth.project) headers['x-goog-user-project'] = auth.project;
    const req = lib.request(
      {
        method: 'POST',
        hostname: u.hostname,
        port: u.port || (u.protocol === 'https:' ? 443 : 80),
        path: u.pathname + u.search,
        headers,
        timeout: 120000,
      },
      (up) => {
        const chunks = [];
        up.on('data', (c) => chunks.push(c));
        up.on('end', () => {
          resolve({ status: up.statusCode, text: Buffer.concat(chunks).toString('utf8') });
        });
        // Neu khong co dong nay: socket upstream loi giua chung (reset, drop)
        // thi 'error' tren response khong ai nghe -> uncaught exception ->
        // ca process Node chet -> Passenger tra 502 HTML. (Loi 14:34 26/11:
        // app sap dung luc goi Gemini.)
        up.on('error', reject);
      }
    );
    req.on('timeout', () => req.destroy(new Error('upstream timeout')));
    req.on('error', reject);
    req.end(data);
  });
}

// Goi stream Gemini. Neu upstream tra loi khong 2xx thi tra { status, errorText }
// (chua gui gi cho client). Neu 2xx thi goi onLine(line) cho moi dong SSE tho
// roi tra { status } khi luong dong.
function callGeminiStream(path, bodyObj, auth, onLine) {
  return new Promise((resolve, reject) => {
    const u = new URL(GEMINI_BASE + path);
    const data = JSON.stringify(bodyObj);
    const lib = u.protocol === 'https:' ? https : http;
    const headers = {
      'Content-Type': 'application/json',
      'Content-Length': Buffer.byteLength(data),
      Authorization: 'Bearer ' + auth.token,
    };
    if (auth.project) headers['x-goog-user-project'] = auth.project;
    const req = lib.request(
      {
        method: 'POST',
        hostname: u.hostname,
        port: u.port || (u.protocol === 'https:' ? 443 : 80),
        path: u.pathname + u.search,
        headers,
        timeout: 120000,
      },
      (up) => {
        const status = up.statusCode;
        up.on('error', reject);
        if (status < 200 || status >= 300) {
          const chunks = [];
          up.on('data', (c) => chunks.push(c));
          up.on('end', () => {
            resolve({ status, errorText: Buffer.concat(chunks).toString('utf8') });
          });
          return;
        }
        let buf = '';
        up.on('data', (c) => {
          buf += c.toString('utf8');
          let idx;
          while ((idx = buf.indexOf('\n')) !== -1) {
            const line = buf.slice(0, idx);
            buf = buf.slice(idx + 1);
            onLine(line);
          }
        });
        up.on('end', () => {
          if (buf.trim() !== '') onLine(buf);
          resolve({ status });
        });
      }
    );
    req.on('timeout', () => req.destroy(new Error('upstream timeout')));
    req.on('error', reject);
    req.end(data);
  });
}

function readBody(req) {
  return new Promise((resolve, reject) => {
    const chunks = [];
    let size = 0;
    req.on('data', (c) => {
      size += c.length;
      if (size > MAX_BODY) {
        reject(new Error('body too large'));
        req.destroy();
        return;
      }
      chunks.push(c);
    });
    req.on('end', () => resolve(Buffer.concat(chunks).toString('utf8')));
    req.on('error', reject);
  });
}

function clientIp(req) {
  const fwd = req.headers['x-forwarded-for'];
  if (typeof fwd === 'string' && fwd.trim() !== '') {
    return fwd.split(',')[0].trim();
  }
  return req.socket.remoteAddress || 'unknown';
}

async function handleChat(req, res) {
  // LOGIN-ONLY: can Bearer token tu client; khong co -> 401.
  const authHeader = req.headers.authorization || '';
  const m = /^Bearer\s+(.+)$/.exec(authHeader.trim());
  const token = m ? m[1].trim() : '';
  if (!token) {
    sendJson(res, 401, { error: 'login required' });
    return;
  }
  // x-goog-user-project: env server uu tien, neu khong co thi lay header
  // do client gui (app lay tu vsix contribution).
  const headerProject = (req.headers['x-goog-user-project'] || '').trim();
  const auth = { token, project: projectNumber || headerProject };
  if (rateLimited(clientIp(req))) {
    sendJson(res, 429, { error: 'rate limited, try again in a minute' });
    return;
  }
  let raw;
  try {
    raw = await readBody(req);
  } catch (e) {
    sendJson(res, 400, { error: String(e.message || e) });
    return;
  }
  let body;
  try {
    body = JSON.parse(raw);
  } catch (e) {
    sendJson(res, 400, { error: 'invalid json' });
    return;
  }
  const model = body.model && String(body.model).trim() !== '' ? String(body.model) : DEFAULT_MODEL;
  const messages = Array.isArray(body.messages) ? body.messages : [];
  const { contents, system } = normalizeContents(messages);
  if (contents.length === 0) {
    sendJson(res, 400, { error: 'no user message' });
    return;
  }
  const geminiBody = {
    contents,
    ...(system !== null
      ? { systemInstruction: { parts: [{ text: system }] } }
      : {}),
  };

  if (body.stream) {
    await handleStream(res, model, geminiBody, auth);
    return;
  }

  let up;
  try {
    up = await callGemini(`/models/${encodeURIComponent(model)}:generateContent`, geminiBody, auth);
  } catch (e) {
    sendJson(res, 502, { error: 'upstream unreachable: ' + String(e.message || e) });
    return;
  }
  if (up.status < 200 || up.status >= 300) {
    sendJson(res, 502, { error: `gemini ${up.status}: ${up.text.slice(0, 300)}` });
    return;
  }
  let parsed;
  try {
    parsed = JSON.parse(up.text);
  } catch (e) {
    sendJson(res, 502, { error: 'bad upstream json: ' + String(e.message || e) });
    return;
  }
  const { text, finish } = geminiText(parsed);
  sendJson(res, 200, {
    id: newId(),
    object: 'chat.completion',
    model,
    choices: [
      {
        index: 0,
        message: { role: 'assistant', content: text },
        finish_reason: mapFinish(finish) || 'stop',
      },
    ],
  });
}

async function handleStream(res, model, geminiBody, auth) {
  const id = newId();
  let first = true;
  let sseStarted = false;

  const sendChunk = (chunk) => {
    res.write('data: ' + JSON.stringify(chunk) + '\n\n');
  };
  const startSse = () => {
    if (sseStarted) return;
    sseStarted = true;
    res.writeHead(200, {
      'Content-Type': 'text/event-stream',
      'Cache-Control': 'no-cache',
      Connection: 'keep-alive',
    });
  };

  let up;
  try {
    up = await callGeminiStream(
      `/models/${encodeURIComponent(model)}:streamGenerateContent?alt=sse`,
      geminiBody,
      auth,
      (line) => {
        startSse();
        const t = line.trim();
        if (!t.startsWith('data:')) return;
        const data = t.slice(5).trim();
        if (data === '[DONE]') return;
        let parsed;
        try {
          parsed = JSON.parse(data);
        } catch (e) {
          return;
        }
        const { text, finish } = geminiText(parsed);
        if (text === '' && finish === null) return;
        sendChunk({
          id,
          object: 'chat.completion.chunk',
          model,
          choices: [
            {
              index: 0,
              delta: {
                ...(first ? { role: 'assistant' } : {}),
                ...(text !== '' ? { content: text } : {}),
              },
              finish_reason: mapFinish(finish),
            },
          ],
        });
        first = false;
      }
    );
  } catch (e) {
    if (!sseStarted) {
      sendJson(res, 502, { error: 'upstream unreachable: ' + String(e.message || e) });
    } else {
      res.write('data: ' + JSON.stringify({ error: 'upstream unreachable: ' + String(e.message || e) }) + '\n\n');
      res.end();
    }
    return;
  }
  if (up.status < 200 || up.status >= 300) {
    // Loi upstream truoc khi co SSE: tra JSON 502 nhu ban Rust.
    if (!sseStarted) {
      sendJson(res, 502, { error: `gemini ${up.status}: ${(up.errorText || '').slice(0, 300)}` });
    } else {
      res.write('data: ' + JSON.stringify({ error: `gemini ${up.status}: ${(up.errorText || '').slice(0, 300)}` }) + '\n\n');
      res.end();
    }
    return;
  }
  startSse();
  res.write('data: [DONE]\n\n');
  res.end();
}

// Luoi cuoi: bat cu loi nao con sot (de log) thay vi de process chet.
process.on('uncaughtException', (e) => {
  console.error('[uncaught]', e && e.stack ? e.stack : e);
});
process.on('unhandledRejection', (e) => {
  console.error('[unhandledRejection]', e && e.stack ? e.stack : e);
});

// Vercel serverless: export handler thay vi http.createServer + listen.
// vercel.json rewrite /v1/chat/completions -> /api/chat nen moi POST toi
// day deu la chat; GET tra 404 JSON (health co ham rieng api/health.js).
module.exports = (req, res) => {
  // Client ngat ket noi giua chung -> res.write co the phat 'error'.
  res.on('error', () => {});
  if (req.method === 'POST') {
    handleChat(req, res).catch((e) => {
      console.error('[error]', e);
      if (!res.headersSent) sendJson(res, 500, { error: 'internal error' });
      else res.end();
    });
    return;
  }
  sendJson(res, 404, { error: 'not found' });
};
