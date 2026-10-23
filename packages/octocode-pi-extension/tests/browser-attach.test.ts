import { describe, expect, it } from 'vitest';
import http from 'node:http';
import type { AddressInfo } from 'node:net';
import { checkDevtoolsUrl, openTab, pageTargets } from '../src/browser/devtools.js';
import { openBrowser } from '../src/browser/session.js';
import { addressOf } from './doubles.js';

describe('the DevTools attach check', () => {
  it.each(['ws://127.0.0.1:9222/devtools/page/ABC', 'ws://localhost:9222/devtools/browser/x', 'ws://[::1]:9222/devtools/page/1'])('accepts %s', (url) => {
    expect(() => checkDevtoolsUrl(url, 9222)).not.toThrow();
  });

  it.each([
    ['a non-ws scheme', 'wss://127.0.0.1:9222/devtools/page/A', /scheme wss: is not ws:/],
    ['an http scheme', 'http://127.0.0.1:9222/devtools/page/A', /scheme http: is not ws:/],
    ['a foreign host', 'ws://evil.example:9222/devtools/page/A', /host evil\.example is not this machine/],
    ['a private network host', 'ws://10.0.0.5:9222/devtools/page/A', /host 10\.0\.0\.5 is not this machine/],
    ['a wrong port', 'ws://127.0.0.1:9333/devtools/page/A', /port 9333 is not 9222/],
    ['no port', 'ws://127.0.0.1/devtools/page/A', /port \(default\) is not 9222/],
    ['a wrong path', 'ws://127.0.0.1:9222/other', /path \/other is not under \/devtools\//],
    ['a path that only starts alike', 'ws://127.0.0.1:9222/devtoolsx/page', /is not under \/devtools\//],
  ])('refuses %s, in every mode', (_name, url, message) => {
    expect(() => checkDevtoolsUrl(url, 9222)).toThrow(message);
  });

  it('refuses a string that is not a URL', () => {
    expect(() => checkDevtoolsUrl('not a url', 9222)).toThrow(/not a URL/);
  });

  it('refuses a browser whose target list points the session at another host, before any socket opens', async () => {
    let upgraded = false;
    const server = http.createServer((_request, response) => {
      const port = (server.address() as AddressInfo).port;
      response.writeHead(200, { 'content-type': 'application/json' }).end(JSON.stringify([{ id: 'a', type: 'page', webSocketDebuggerUrl: `ws://evil.example:${port}/devtools/page/a` }]));
    }).on('upgrade', () => void (upgraded = true));
    await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve));
    try {
      await expect(openBrowser({}, { port: (server.address() as AddressInfo).port, reuseTab: true })).rejects.toThrow(/Refused the DevTools URL ws:\/\/evil\.example:\d+\/devtools\/page\/a: host evil\.example is not this machine/);
      expect(upgraded).toBe(false);
    } finally {
      server.close();
    }
  });
});

/** A browser endpoint answering every request with `body`. */
async function answering(body: unknown): Promise<{ port: number; close: () => void }> {
  const server = http.createServer((_request, response) => response.writeHead(200, { 'content-type': 'application/json' }).end(JSON.stringify(body)));
  await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve));
  return { port: addressOf(server).port, close: () => server.close() };
}

describe('the DevTools target endpoints', () => {
  it('lists only the page targets that carry an id and a DevTools URL, in order, as Chrome sent them', async () => {
    const ws = 'ws://127.0.0.1:1/devtools/page/';
    const page = { id: 'a', type: 'page', title: 'A', url: 'https://a.example/', webSocketDebuggerUrl: `${ws}a` };
    const browser = await answering([page, { id: 'w', type: 'service_worker', webSocketDebuggerUrl: `${ws}w` }, { id: 'n', type: 'page' }, { type: 'page', webSocketDebuggerUrl: `${ws}x` }, null, 'tab', { id: 'b', type: 'page', webSocketDebuggerUrl: `${ws}b` }]);
    try {
      expect(await pageTargets(browser.port)).toEqual([page, { id: 'b', type: 'page', webSocketDebuggerUrl: `${ws}b` }]);
    } finally {
      browser.close();
    }
  });

  it('refuses a target list that is not a list', async () => {
    const browser = await answering({ id: 'a' });
    try {
      await expect(pageTargets(browser.port)).rejects.toThrow(/answered \/json\/list without a target list/);
    } finally {
      browser.close();
    }
  });

  it('returns the opened tab, and refuses one without an id and DevTools URL', async () => {
    const opened = { id: 't', type: 'page', webSocketDebuggerUrl: 'ws://127.0.0.1:1/devtools/page/t' };
    const good = await answering(opened);
    const bad = await answering({ id: 't' });
    try {
      expect(await openTab(good.port, 'about:blank')).toEqual(opened);
      await expect(openTab(bad.port, 'about:blank')).rejects.toThrow(/opened a tab without an id and DevTools URL/);
    } finally {
      good.close();
      bad.close();
    }
  });
});
