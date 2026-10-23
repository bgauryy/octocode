// Local HTTP fixtures for the live suite: generated network, frame and stealth routes, then
// static files from tests/fixtures. Runs in a child process because the checks block the
// suite's event loop with spawnSync.
import { createServer } from 'node:http';
import { existsSync, readFileSync } from 'node:fs';
import { join } from 'node:path';

const html = (res, body) => {
  res.setHeader('content-type', 'text/html');
  res.end(body);
};
const crossOrigin = server => `http://localhost:${server.address().port}`;

/** Generated routes by pathname; each answers the request itself. */
const routes = {
  '/stream': res => {
    res.setHeader('content-type', 'text/plain');
    res.flushHeaders();
    res.write('Content ');
    setTimeout(() => res.end('loaded'), 700);
  },
  '/redirect': res => {
    res.writeHead(302, { location: '/stream' });
    res.end();
  },
  '/pending': res => {
    res.setHeader('content-type', 'text/plain');
    res.flushHeaders();
    setTimeout(() => res.end('late'), 2500);
  },
  '/network-flow': res =>
    html(
      res,
      '<title>Network flow</title><script>fetch("/redirect");fetch("/pending");</script>'
    ),
  '/frame-network-flow': (res, { server }) =>
    html(
      res,
      `<title>Frame network flow</title><iframe src="${crossOrigin(server)}/network-child"></iframe>`
    ),
  '/network-child': res =>
    html(
      res,
      '<title>Network child</title><script>setTimeout(()=>fetch("/stream"),300)</script>'
    ),
  '/cross-frame': (res, { server }) =>
    html(
      res,
      `<title>Cross-origin frame</title><h1>Cross-origin frame</h1><iframe title="Payment" src="${crossOrigin(server)}/cross-frame.html" width="400" height="120"></iframe>`
    ),
  '/stealth-flow': (res, { server }) =>
    html(
      res,
      `<title>Stealth flow</title><iframe src="${crossOrigin(server)}/stealth-frame"></iframe><script>window.__worker=new Worker('/stealth-worker.js');</script>`
    ),
  '/stealth-frame': (res, { req }) =>
    html(
      res,
      `<title>Stealth frame</title><script>window.__seen={ua:navigator.userAgent,platform:navigator.platform,header:${JSON.stringify(String(req.headers['user-agent']))}};</script>`
    ),
  '/stealth-worker.js': res => {
    res.setHeader('content-type', 'text/javascript');
    res.end(
      'onmessage=()=>postMessage({ua:navigator.userAgent,platform:navigator.platform})'
    );
  },
  '/done': (res, { url }) =>
    html(
      res,
      `<title>Done</title><h1>Done ${String(url.searchParams.get('q')).replace(/[<&]/g, '')}</h1>`
    ),
};

export function createFixtureServer(fixtures) {
  const server = createServer((req, res) => {
    const url = new URL(req.url, 'http://fixture');
    const route = routes[url.pathname];
    if (route) {
      route(res, { req, url, server });
      return;
    }
    const file = join(
      fixtures,
      url.pathname.replace(/^\/+/, '') || 'actions.html'
    );
    if (!file.startsWith(fixtures) || !existsSync(file)) {
      res.statusCode = 404;
      res.end('not found');
      return;
    }
    html(res, readFileSync(file));
  });
  return server;
}
