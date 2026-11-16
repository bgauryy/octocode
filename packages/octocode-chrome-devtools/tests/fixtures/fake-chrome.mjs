#!/usr/bin/env node
// Browser-free CDP listener for cleanup tests. Its command line carries the same
// --remote-debugging-port/--user-data-dir flags as a launched Chrome, and it
// answers /json/version plus SystemInfo.getProcessInfo with its own pid. It also lists
// the tabs opened through PUT /json/new?<url> on /json (newest or last activated first, like
// Chrome), and serves each tab's page socket: Page.navigate sets the tab URL, the identity probe
// and window geometry of the stealth setup are answered. GET /__log returns every tab creation
// and page command in order, so tests can assert what happened before a document load.
import { createServer } from 'node:http';
import { WebSocketServer } from 'ws';

const flag = name =>
  process.argv
    .find(arg => arg.startsWith(`--${name}=`))
    ?.slice(name.length + 3);
const port = Number(flag('remote-debugging-port'));
const tabs = [];
const log = [];
const server = createServer((req, res) => {
  res.setHeader('content-type', 'application/json');
  if (req.url === '/json') return res.end(JSON.stringify(tabs));
  if (req.url === '/__log') return res.end(JSON.stringify(log));
  if (req.url?.startsWith('/json/close/')) {
    const index = tabs.findIndex(
      tab => tab.id === req.url.slice('/json/close/'.length)
    );
    if (index < 0) return res.writeHead(404).end('No such target id');
    tabs.splice(index, 1);
    res.setHeader('content-type', 'text/plain');
    return res.end('Target is closing');
  }
  if (req.url?.startsWith('/json/activate/')) {
    const index = tabs.findIndex(
      tab => tab.id === req.url.slice('/json/activate/'.length)
    );
    if (index < 0) return res.writeHead(404).end('No such target id');
    tabs.unshift(...tabs.splice(index, 1));
    log.push({ kind: 'activate', id: tabs[0].id });
    res.setHeader('content-type', 'text/plain');
    return res.end('Target activated');
  }
  if (req.url?.startsWith('/json/new?')) {
    const tab = {
      id: `tab-${tabs.length + 1}`,
      type: 'page',
      title: '',
      url: decodeURIComponent(req.url.slice('/json/new?'.length)),
      webSocketDebuggerUrl: `ws://localhost:${port}/devtools/page/${tabs.length + 1}`,
    };
    tabs.unshift(tab);
    log.push({ kind: 'new', url: tab.url, id: tab.id });
    return res.end(JSON.stringify(tab));
  }
  if (req.url !== '/json/version') return res.writeHead(404).end();
  res.end(
    JSON.stringify({
      Browser: 'FakeChrome/1.0',
      webSocketDebuggerUrl: `ws://localhost:${port}/devtools/browser/fake`,
    })
  );
});
const hostProbe = {
  userAgent:
    'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/155.0.0.0 Safari/537.36',
  platform: 'MacIntel',
  metadata: {
    architecture: 'arm',
    bitness: '64',
    brands: [{ brand: 'Chromium', version: '155' }],
    formFactors: ['Desktop'],
    fullVersionList: [],
    mobile: false,
    model: '',
    platform: 'macOS',
    platformVersion: '15.4.0',
    wow64: false,
  },
};

// What a page target answers; anything else succeeds with an empty result.
function pageResult(tab, method, params, socket) {
  if (method === 'Page.navigate') {
    tab.url = params.url;
    setTimeout(
      () => socket.send(JSON.stringify({ method: 'Page.loadEventFired' })),
      5
    );
    return { frameId: tab.id };
  }
  if (method === 'Target.createTarget') return { targetId: 'probe-target' };
  if (method === 'Target.attachToTarget') return { sessionId: 'probe-session' };
  if (method !== 'Runtime.evaluate') return {};
  const { expression } = params;
  if (expression === 'location.href') return { result: { value: tab.url } };
  if (expression.includes('getHighEntropyValues'))
    return { result: { value: hostProbe } };
  if (expression.includes('outerWidth'))
    return {
      result: {
        value: {
          width: 1280,
          height: 720,
          screenWidth: 1920,
          screenHeight: 1080,
        },
      },
    };
  return { result: { value: true } };
}

new WebSocketServer({ server }).on('connection', (socket, req) => {
  const tab = tabs.find(t => t.webSocketDebuggerUrl.endsWith(req.url));
  socket.on('message', data => {
    const { id, method, params = {}, sessionId } = JSON.parse(String(data));
    if (tab) {
      log.push({ kind: 'cdp', method, tab: tab.id, url: params.url });
      return socket.send(
        JSON.stringify({
          id,
          sessionId,
          result: pageResult(tab, method, params, socket),
        })
      );
    }
    socket.send(
      JSON.stringify(
        method === 'SystemInfo.getProcessInfo'
          ? {
              id,
              result: {
                processInfo: [{ type: 'browser', id: process.pid, cpuTime: 0 }],
              },
            }
          : { id, error: { code: -32601, message: 'not found' } }
      )
    );
  });
});
server.listen(port, () => console.log('ready'));
