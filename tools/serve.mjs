// Serves the app under /Routines/, the path GitHub Pages uses, so tests catch
// anything that assumes it lives at the site root. Usage: node tools/serve.mjs [port] [dir]
import { createServer } from 'node:http';
import { readFile, stat } from 'node:fs/promises';
import { extname, join, normalize, resolve } from 'node:path';

const port = Number(process.argv[2] || 8080);
const root = resolve(process.argv[3] || '.');
const PREFIX = '/Routines/';
const TYPES = {
  '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.json': 'application/json',
  '.webmanifest': 'application/manifest+json', '.png': 'image/png', '.svg': 'image/svg+xml',
};

createServer(async (req, res) => {
  const url = new URL(req.url, 'http://x');
  if (!url.pathname.startsWith(PREFIX)) {
    res.writeHead(302, { location: PREFIX }).end();
    return;
  }
  let rel = normalize(decodeURIComponent(url.pathname.slice(PREFIX.length)));
  if (rel.startsWith('..') || rel.split(/[\\/]/)[0] === 'node_modules') {
    res.writeHead(404).end();
    return;
  }
  let file = join(root, rel);
  try {
    if ((await stat(file)).isDirectory()) file = join(file, 'index.html');
    const body = await readFile(file);
    res.writeHead(200, { 'content-type': TYPES[extname(file)] || 'application/octet-stream', 'cache-control': 'no-store' }).end(body);
  } catch {
    res.writeHead(404).end();
  }
}).listen(port, () => console.log(`http://localhost:${port}${PREFIX}`));
