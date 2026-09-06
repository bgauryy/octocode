import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { afterEach, test, vi } from 'vitest';
import { closeConfiguration, openMcpManager } from '../src/tools/mcp-html.js';
import * as opener from '../src/tools/local-url-opener.js';
import { stopLocalServer } from '../src/tools/local-server.js';
import { getFooterDensity, setFooterDensity } from '../src/ui-extras.js';
import { getPermissionLevel, setPermissionLevel } from '../src/tools/approval.js';
import { resetDialStateForTests } from '../src/tools/effort-dial.js';
import type { PiContext, PiInstance } from '../src/types.js';

const originalHome = process.env['OCTOCODE_HOME'];
const originalWorkers = process.env['OCTOCODE_AGENT_MAX_ACTIVE'];
const roots: string[] = [];
afterEach(() => {
  stopLocalServer();
  vi.restoreAllMocks();
  setFooterDensity('compact');
  setPermissionLevel('default');
  resetDialStateForTests();
  if (originalHome === undefined) delete process.env['OCTOCODE_HOME']; else process.env['OCTOCODE_HOME'] = originalHome;
  if (originalWorkers === undefined) delete process.env['OCTOCODE_AGENT_MAX_ACTIVE']; else process.env['OCTOCODE_AGENT_MAX_ACTIVE'] = originalWorkers;
  for (const root of roots.splice(0)) fs.rmSync(root, { recursive: true, force: true });
});

function fixture(): PiContext {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'configuration-flow-'));
  roots.push(root);
  process.env['OCTOCODE_HOME'] = path.join(root, 'home');
  return { cwd: root, hasUI: true, mode: 'tui', isProjectTrusted: () => true } as PiContext;
}

async function tokenFrom(url: string): Promise<string> {
  const response = await fetch(url);
  assert.equal(response.status, 200);
  assert.match(response.headers.get('cache-control') ?? '', /no-store/);
  const html = await response.text();
  assert.match(html, /Your configuration/);
  const token = html.match(/const token = "([a-f0-9]+)"/)?.[1];
  assert.ok(token);
  return token;
}

async function post(url: string, token: string, body: unknown, origin = new URL(url).origin): Promise<Response> {
  return fetch(new URL('__octocode/action', url), {
    method: 'POST',
    headers: { origin, 'content-type': 'application/json', 'x-octocode-action-token': token },
    body: JSON.stringify(body),
  });
}

test('configuration opens the system browser, applies controls, rejects stale actions, and retires on shutdown', async () => {
  const ctx = fixture();
  const themes: string[] = [];
  ctx.ui = { setTheme: (theme: string) => { themes.push(theme); return { success: true }; } } as PiContext['ui'];
  const thinking: string[] = [];
  const pi = { setThinkingLevel: (level: string) => thinking.push(level) } as unknown as PiInstance;
  const open = vi.spyOn(opener, 'openLocalUrl').mockResolvedValue({ ok: true, requested: 'system', openedIn: 'system' });
  const result = await openMcpManager(ctx, [], 'overview', [{ name: 'configuration', description: 'Open configuration', source: 'extension', sourceInfo: { path: '/extension/index.ts', source: 'octocode', scope: 'temporary', origin: 'package' } }], pi);
  assert.equal(result.ok, true);
  assert.ok(result.url);
  assert.equal(open.mock.calls[0]?.[1]?.preference, 'system');
  const token = await tokenFrom(result.url);
  assert.equal((await post(result.url, 'wrong', { action: 'set-footer-density', density: 'full' })).status, 403);
  assert.equal((await post(result.url, token, { action: 'set-footer-density', density: 'full' }, 'https://attacker.invalid')).status, 403);
  const first = await post(result.url, token, { action: 'set-footer-density', density: 'full', expectedRevision: '0' });
  assert.equal(first.status, 200, await first.text());
  assert.equal(getFooterDensity(), 'full');
  const stale = await post(result.url, token, { action: 'set-theme', theme: 'dark', expectedRevision: '0' });
  assert.equal(stale.status, 400);
  assert.equal(themes.length, 0, 'stale actions cannot change host state');
  assert.equal((await post(result.url, token, { action: 'set-theme', theme: 'light', expectedRevision: '1' })).status, 200);
  assert.equal(themes.length, 1);
  assert.equal((await post(result.url, token, { action: 'set-effort', level: 'high', expectedRevision: '2' })).status, 200);
  assert.equal(thinking.length, 1);
  assert.equal((await post(result.url, token, { action: 'set-permission-level', level: 'strict', expectedRevision: '3' })).status, 200);
  assert.equal(getPermissionLevel(), 'strict');
  const reopened = await openMcpManager(ctx, [], 'overview', [], pi);
  assert.ok(reopened.url);
  assert.notEqual(await tokenFrom(reopened.url), token);
  assert.equal((await post(reopened.url, token, { action: 'set-footer-density', density: 'compact' })).status, 403);
  closeConfiguration(ctx);
  assert.equal((await fetch(reopened.url)).status, 404);
});

test('configuration exposes a working manual URL when opening the browser fails', async () => {
  const ctx = fixture();
  vi.spyOn(opener, 'openLocalUrl').mockResolvedValue({ ok: false, requested: 'system', openedIn: 'none', message: 'Browser unavailable' });
  const opened = await openMcpManager(ctx);
  assert.equal(opened.ok, false);
  assert.equal(opened.message, 'Browser unavailable');
  assert.ok(opened.url);
  const token = await tokenFrom(opened.url);
  const unavailable = await post(opened.url, token, { action: 'set-effort', level: 'high', expectedRevision: '0' });
  assert.equal(unavailable.status, 400);
  assert.match(await unavailable.text(), /unavailable in this host/);
  const html = await (await fetch(opened.url)).text();
  assert.match(html, /data-action="set-theme" data-theme="dark" aria-pressed="false" disabled/);
  closeConfiguration(ctx);
});
