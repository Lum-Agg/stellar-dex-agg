import { createServer } from 'node:http';
import { readFile, stat } from 'node:fs/promises';
import { resolve, extname, sep } from 'node:path';

const root = resolve('out-e2e');
const mime = {
  '.html': 'text/html',
  '.js': 'text/javascript',
  '.css': 'text/css',
  '.json': 'application/json',
  '.svg': 'image/svg+xml',
  '.woff2': 'font/woff2',
  '.png': 'image/png',
  '.ico': 'image/x-icon',
  '.txt': 'text/plain',
};

// Test-only static export server. Never proxy an API request to a real service.
createServer(async (request, response) => {
  try {
    const pathname = decodeURIComponent(new URL(request.url, 'http://127.0.0.1').pathname);
    const base = resolve(root, `.${pathname}`);
    if (base !== root && !base.startsWith(`${root}${sep}`)) {
      response.writeHead(403).end();
      return;
    }
    for (const file of [base, `${base}.html`, resolve(base, 'index.html')]) {
      if (!(await stat(file).catch(() => null))?.isFile()) continue;
      response.writeHead(200, {
        'Content-Type': mime[extname(file)] ?? 'application/octet-stream',
      });
      response.end(await readFile(file));
      return;
    }
    response.writeHead(404).end();
  } catch {
    response.writeHead(400).end();
  }
}).listen(3105, '127.0.0.1');
