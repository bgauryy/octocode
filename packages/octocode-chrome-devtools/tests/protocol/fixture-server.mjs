// Local HTTP and WebSocket fixtures for the protocol suite, served from a child process
// (`protocol-suite.mjs --serve`) so the suite's blocking engine calls never stall them.
import { createServer } from 'node:http';
import { createHash } from 'node:crypto';

/** Routes by URL pathname; each handler answers the request itself. */
const routes = {
  '/isolated-parent': (res, { server }) => {
    res.setHeader('content-type', 'text/html');
    res.end(
      `<!doctype html><title>Cross site parent</title><iframe id="isolated" src="http://localhost:${server.address().port}/isolated?generation=1"></iframe><button id="replace" onclick="document.querySelector('#isolated').outerHTML='<iframe id=isolated src=http://localhost:${server.address().port}/isolated?generation=2></iframe>'">Replace isolated</button>`
    );
  },
  '/isolated': (res, { route }) => {
    res.setHeader('content-type', 'text/html');
    res.end(
      `<!doctype html><p id="ready">Isolated generation ${route.searchParams.get('generation')}</p><button id="advance" onclick="document.querySelector('#ready').textContent='Isolated clicked'">Advance isolated</button>`
    );
  },
  '/owned-sw.js': res => {
    res.setHeader('content-type', 'text/javascript');
    res.setHeader('service-worker-allowed', '/sw/');
    res.end(
      `self.addEventListener('install',e=>e.waitUntil(self.skipWaiting()));self.addEventListener('activate',e=>e.waitUntil(self.clients.claim()));self.addEventListener('fetch',e=>{if(new URL(e.request.url).pathname==='/sw/response')e.respondWith(new Response(JSON.stringify({source:'service-worker',message:'owned'}),{headers:{'content-type':'application/json'}}))});`
    );
  },
  '/sw/response': res => {
    res.setHeader('content-type', 'application/json');
    res.end(JSON.stringify({ source: 'network', message: 'owned' }));
  },
  '/sw/page': res => {
    res.setHeader('content-type', 'text/html');
    res.end(
      '<!doctype html><title>Owned worker client</title><p id="ready">Worker client ready</p>'
    );
  },
  '/stream-download': res => {
    res.setHeader('content-type', 'application/octet-stream');
    res.setHeader(
      'content-disposition',
      'attachment; filename="owned-stream.bin"'
    );
    let offset = 0;
    const total = 2 * 1024 * 1024;
    const timer = setInterval(() => {
      const chunk = Buffer.alloc(Math.min(32768, total - offset));
      for (let i = 0; i < chunk.length; i++) chunk[i] = (offset + i) % 251;
      res.write(chunk);
      offset += chunk.length;
      if (offset === total) {
        clearInterval(timer);
        res.end();
      }
    }, 10);
    res.on('close', () => clearInterval(timer));
  },
  '/redirect-owned': res => {
    res.writeHead(302, { location: '/ready-late' });
    res.end();
  },
  '/ready-late': res => {
    res.setHeader('content-type', 'text/html');
    res.end(`<!doctype html><h1 id="heading">Results</h1><section id="rows"></section><input id="visible" aria-label="Search owned"><input hidden aria-label="Search owned"><div id="closed"></div><script>
      window.initialRows=document.querySelectorAll('.record').length;
      setTimeout(()=>{document.querySelector('#rows').innerHTML='<p class=record>Owned record one</p><p class=record>Owned record two</p>'},350);
      const root=document.querySelector('#closed').attachShadow({mode:'closed'});root.innerHTML='<button id=closedButton>Closed action</button>';
    </script>`);
  },
  '/download': res => {
    res.setHeader('content-type', 'text/plain');
    res.setHeader(
      'content-disposition',
      'attachment; filename="owned-fixture.txt"'
    );
    res.end('owned download evidence\n');
  },
  '/advanced': res => {
    res.setHeader('content-type', 'text/html');
    res.end(`<!doctype html><title>Advanced owned fixture</title>
      <button id="replaceOuter" onclick="document.querySelector('#outer').outerHTML='<iframe id=outer title=Outer src=/outer?generation=2></iframe>'">Replace outer</button>
      <iframe id="outer" title="Outer" src="/outer?generation=1"></iframe>
      <div id="shadow"></div><output id="shadowResult">Shadow idle</output>
      <button class="duplicate" onclick="document.body.dataset.duplicate='first'">Duplicate action</button>
      <button class="duplicate" onclick="document.body.dataset.duplicate='second'">Duplicate action</button>
      <label>Owned upload <input id="upload" type="file" onchange="document.querySelector('#uploadResult').textContent=this.files[0].name+':'+this.files[0].size"></label><output id="uploadResult">Upload idle</output>
      <a id="download" href="/download">Download fixture</a>
      <output id="dialogResult">Dialog idle</output>
      <script>const shadow=document.querySelector('#shadow').attachShadow({mode:'open'});shadow.innerHTML='<button id="shadowButton">Shadow action</button>';shadow.querySelector('button').onclick=()=>document.querySelector('#shadowResult').textContent='Shadow clicked';</script>`);
  },
  '/outer': (res, { route }) => {
    const generation = route.searchParams.get('generation');
    res.setHeader('content-type', 'text/html');
    res.end(
      `<!doctype html><title>Outer ${generation}</title><p>Outer generation ${generation}</p><button id="replaceInner" onclick="document.querySelector('#inner').outerHTML='<iframe id=inner title=Inner src=/inner?generation=3></iframe>'">Replace inner</button><iframe id="inner" title="Inner" src="/inner?generation=${generation}"></iframe>`
    );
  },
  '/inner': (res, { route }) => {
    res.setHeader('content-type', 'text/html');
    res.end(
      `<!doctype html><title>Owned ${route.pathname.slice(1)}</title><p id="ready">${route.pathname === '/popup' ? 'Popup ready' : 'Inner generation ' + route.searchParams.get('generation')}</p>`
    );
  },
  '/popup': (res, context) => routes['/inner'](res, context),
};
// dev-check fixture: a warning, a source-mapped console error and exception, a 404 and a refused request.
const devRoutes = {
  '/dev-check': res => {
    res.setHeader('content-type', 'text/html');
    res.end(
      `<!doctype html><title>Dev check</title><p id="ready">Dev page</p><script>console.warn('dev warning')</script><script src="/dev-check/app.js"></script><script>fetch('/dev-check/missing').catch(()=>{});fetch('http://127.0.0.1:9/refused').catch(()=>{})</script>`
    );
  },
  '/dev-check/app.js': res => {
    res.setHeader('content-type', 'text/javascript');
    res.end(
      "function boot() {\n  console.error('dev boot failed');\n  throw new Error('dev boom');\n}\nsetTimeout(boot, 0);\n//# sourceMappingURL=app.js.map\n"
    );
  },
  '/dev-check/app.js.map': res => {
    res.setHeader('content-type', 'application/json');
    res.end(
      JSON.stringify({
        version: 3,
        sources: ['src/app.ts'],
        names: [],
        mappings: 'AAAA;AACA;AACA;AACA;AACA',
      })
    );
  },
  '/dev-check/missing': res => {
    res.statusCode = 404;
    res.end('missing');
  },
};
Object.assign(routes, devRoutes);

/** Routes by the exact request URL, query included. */
const exactRoutes = {
  '/worker.js': res => {
    res.setHeader('content-type', 'text/javascript');
    res.end('self.onmessage=e=>postMessage(e.data);');
  },
  '/data': res => {
    res.setHeader('content-type', 'application/json');
    res.end('{"message":"Data ready"}');
  },
};

const auditPage = res => {
  res.setHeader('content-type', 'text/html');
  res.end(
    '<!doctype html><title>CDP audit fixture</title><style>#q{color:rgb(1,2,3)}</style><label>Search <input id="q"></label><button onclick="document.querySelector(\'#out\').textContent=\'Clicked\'">Go</button><output id="out">Ready</output>'
  );
};

/** Answers every WebSocket upgrade with one text frame, "hello". */
function upgrade(req, socket) {
  const accept = createHash('sha1')
    .update(
      req.headers['sec-websocket-key'] + '258EAFA5-E914-47DA-95CA-C5AB0DC85B11'
    )
    .digest('base64');
  socket.write(
    'HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: ' +
      accept +
      '\r\n\r\n'
  );
  socket.write(Buffer.concat([Buffer.from([0x81, 5]), Buffer.from('hello')]));
  socket.on('error', async () => {});
}

export function createFixtureServer() {
  const server = createServer((req, res) => {
    const route = new URL(req.url, 'http://fixture');
    const handler = routes[route.pathname] ?? exactRoutes[req.url] ?? auditPage;
    handler(res, { req, route, server });
  });
  server.on('upgrade', upgrade);
  return server;
}
