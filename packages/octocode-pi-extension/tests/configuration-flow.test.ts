import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { afterEach, test, vi } from 'vitest';
import { closeConfiguration, openMcpManager } from '../src/tools/mcp/html.js';
import { discoverSkillCandidates, discoverSkills } from '../src/tools/skill-discovery.js';
import * as mcpTools from '../src/tools/mcp-tool.js';
import * as opener from '../src/tools/local-url-opener.js';
import { stopLocalServer } from '../src/tools/local-server.js';
import { getFooterDensity, setFooterDensity } from '../src/ui-extras.js';
import { getPermissionLevel } from '../src/tools/approval.js';
import { resetDialStateForTests } from '../src/tools/effort-dial.js';
import type { PiContext, PiInstance } from '../src/types.js';

const originalHome = process.env['OCTOCODE_HOME'];
const originalWorkers = process.env['OCTOCODE_AGENT_MAX_ACTIVE'];
const roots: string[] = [];
afterEach(() => {
  stopLocalServer();
  vi.restoreAllMocks();
  setFooterDensity('compact');
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
  for (const script of html.matchAll(/<script>([\s\S]*?)<\/script>/g)) assert.doesNotThrow(() => new Function(script[1]!), 'configuration event handlers must be valid JavaScript');
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
  assert.equal(getPermissionLevel(ctx), 'strict');
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

test('browser links an exact skill revision and rejects a changed source before mutation', async () => {
  const ctx = fixture();
  const skillPath = path.join(ctx.cwd!, '.claude', 'skills', 'import-review', 'SKILL.md');
  fs.mkdirSync(path.dirname(skillPath), { recursive: true });
  const body = '---\nname: import-review\ndescription: Imported review workflow.\n---\nUse exact evidence.\n';
  fs.writeFileSync(skillPath, body);
  vi.spyOn(opener, 'openLocalUrl').mockResolvedValue({ ok: true, requested: 'system', openedIn: 'system' });
  vi.spyOn(mcpTools, 'refreshMcpCapabilities').mockResolvedValue();
  const candidate = discoverSkillCandidates(ctx.cwd!, [], undefined, { trusted: true }).find(skill => skill.path === skillPath)!;
  assert.ok(candidate.sourceId && candidate.revision);
  assert.equal(discoverSkills(ctx.cwd!).some(skill => skill.name === 'import-review'), false);
  const opened = await openMcpManager(ctx, []);
  const html = await (await fetch(opened.url!)).text();
  assert.match(html, /Review complete skill instructions/);
  assert.doesNotMatch(html, /Use exact evidence\./, 'skill bodies load on demand, not inline');
  const stamp = html.match(/capabilityRevision:"(sha256:[a-f0-9]{64})"/)?.[1];
  assert.ok(stamp, 'browser emits a concrete capability review stamp');
  const token = await tokenFrom(opened.url!);
  const preview = await post(opened.url!, token, { action: 'preview-skill', source: candidate.sourceId, hash: candidate.revision });
  assert.equal(preview.status, 200);
  assert.equal(((await preview.json()) as { value: { preview: string } }).value.preview, body, 'review loads the exact source bytes');
  fs.writeFileSync(skillPath, body + 'Changed after opening the page.\n');
  const stalePreview = await post(opened.url!, token, { action: 'preview-skill', source: candidate.sourceId, hash: candidate.revision });
  assert.equal(stalePreview.status, 400);
  assert.match(await stalePreview.text(), /Source changed/);
  const stale = await post(opened.url!, token, { action: 'review-skill', source: candidate.sourceId, hash: candidate.revision, scope: 'project', capabilityRevision: stamp });
  assert.equal(stale.status, 400);
  assert.equal(discoverSkills(ctx.cwd!).some(skill => skill.name === 'import-review'), false);
  fs.writeFileSync(skillPath, body);
  const accepted = await post(opened.url!, token, { action: 'review-skill', source: candidate.sourceId, hash: candidate.revision, scope: 'project', capabilityRevision: stamp });
  assert.equal(accepted.status, 200, await accepted.text());
  assert.equal(fs.readFileSync(skillPath, 'utf8'), body, 'import is a link; source remains untouched');
  assert.equal(discoverSkills(ctx.cwd!).filter(skill => skill.name === 'import-review').length, 1);
  const page = await (await fetch(opened.url!)).text();
  assert.equal((page.match(/<h3>import-review<\/h3>/g) ?? []).length, 1);
  closeConfiguration(ctx);
});
