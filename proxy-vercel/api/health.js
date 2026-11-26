// GET /health -> "ok" (qua rewrite trong vercel.json).
module.exports = (req, res) => {
  res.on('error', () => {});
  res.writeHead(200, { 'Content-Type': 'text/plain' });
  res.end('ok');
};
