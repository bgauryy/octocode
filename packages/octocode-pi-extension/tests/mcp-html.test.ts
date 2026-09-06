import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { afterEach, test } from 'vitest';
import { SETTINGS_HTML_FILE, applyMcpManagerAction, parseMcpManagerAction, renderMcpManagerPage } from '../src/tools/mcp-html.js';
import type { PiCommand, PiContext } from '../src/types.js';
import { getFooterDensity, setFooterDensity } from '../src/ui-extras.js';
import { getPermissionLevel, setPermissionLevel } from '../src/tools/approval.js';
import { projectMcpPath } from '../src/tools/mcp-config.js';

const originalHome = process.env['OCTOCODE_HOME'];
const originalCompactMcp = process.env['OCTOCODE_COMPACT_MCP'];
const roots: string[] = [];
afterEach(() => {
  if (originalHome === undefined) delete process.env['OCTOCODE_HOME'];
  else process.env['OCTOCODE_HOME'] = originalHome;
  if (originalCompactMcp === undefined) delete process.env['OCTOCODE_COMPACT_MCP'];
  else process.env['OCTOCODE_COMPACT_MCP'] = originalCompactMcp;
  while (roots.length) fs.rmSync(roots.pop()!, { recursive: true, force: true });
  setFooterDensity('default');
  setPermissionLevel('default');
});

test('settings actions apply typed session runtime controls', async () => {
  const density = parseMcpManagerAction({ action: 'set-footer-density', density: 'compact' });
  const permission = parseMcpManagerAction({ action: 'set-permission-level', level: 'strict' });
  await applyMcpManagerAction(density);
  await applyMcpManagerAction(permission);
  assert.equal(getFooterDensity(), 'compact');
  assert.equal(getPermissionLevel(), 'strict');
  assert.throws(() => parseMcpManagerAction({ action: 'set-footer-density', density: 'huge' }), /Invalid footer density/);
  assert.throws(() => parseMcpManagerAction({ action: 'set-permission-level', level: 'unsafe' }), /Invalid permission level/);
});

test('settings actions use canonical optimistic revisions and redacted provenance', async () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'octo-canonical-settings-'));
  roots.push(root);
  const ctx = { cwd: root } as unknown as PiContext;
  await applyMcpManagerAction(parseMcpManagerAction({ action: 'set-footer-density', density: 'compact', expectedRevision: '0' }), ctx);
  await assert.rejects(
    applyMcpManagerAction(parseMcpManagerAction({ action: 'set-footer-density', density: 'full', expectedRevision: '0' }), ctx),
    /changed since snapshot/i,
  );
  const html = await renderMcpManagerPage(ctx);
  assert.match(html, /data-settings-revision="1"/);
  assert.match(html, /Footer density/);
  assert.match(html, /data-settings-revision="1"/);
});

test('hook review actions bind settings.html to the canonical exact-hash catalog', async () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'octo-hook-settings-'));
  roots.push(root);
  const hookDir = path.join(root, '.codex');
  fs.mkdirSync(hookDir, { recursive: true });
  fs.writeFileSync(path.join(hookDir, 'hooks.json'), JSON.stringify({ hooks: {
    PreToolUse: [{ matcher: '^write$', hooks: [{ type: 'command', command: './check.sh' }] }],
  } }));
  const ctx = { cwd: root, isProjectTrusted: () => true } as unknown as PiContext;
  const before = await renderMcpManagerPage(ctx);
  const review = before.match(/data-action="review-hook" data-source="([^"]+)" data-hash="([a-f0-9]{64})"/);
  const hookRevision = before.match(/id="hooks"[\s\S]*?<span>revision ([^<]+)<\/span>/)?.[1];
  assert.ok(review);
  assert.ok(hookRevision);
  await applyMcpManagerAction(parseMcpManagerAction({ action: 'review-hook', source: review[1], hash: review[2], expectedRevision: hookRevision }), ctx);
  const after = await renderMcpManagerPage(ctx);
  assert.match(after, /badge on">trusted/);
  assert.equal(after.includes(`data-action="review-hook" data-source="${review[1]}" data-hash="${review[2]}"`), false);
  assert.throws(() => parseMcpManagerAction({ action: 'review-hook', source: review[1], hash: 'bad' }), /Invalid hook review hash/);
});

test('MCP manager action schema accepts references and rejects raw secret fields', () => {
  const action = parseMcpManagerAction({
    action: 'add',
    server: 'docs',
    scope: 'global',
    config: { url: 'https://mcp.example.test/api', headerRefs: { Authorization: 'DOCS_AUTH' } },
  });
  assert.equal(action.action, 'add');
  assert.throws(() => parseMcpManagerAction({
    action: 'add',
    server: 'docs',
    scope: 'global',
    config: { url: 'https://mcp.example.test/api', headers: { Authorization: 'Bearer secret' } },
  }), /Unsupported MCP config field: headers/);
  assert.throws(() => parseMcpManagerAction({ action: 'retry', server: 'docs', scope: 'invalid' }), /Invalid MCP scope/);
  assert.throws(() => parseMcpManagerAction({ action: 'retry', server: 'docs', unexpected: true }), /Unsupported settings action field/);
  assert.throws(() => parseMcpManagerAction({
    action: 'add',
    server: 'docs',
    config: { command: 'node', envRefs: { 'BAD-NAME': 'DOCS_AUTH' } },
  }), /Invalid envRefs destination/);
});

test('settings action schema and SQLite handler support workspace/global skill enablement', async () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'octo-skill-html-action-'));
  roots.push(root);
  process.env['OCTOCODE_HOME'] = path.join(root, 'home');
  const cwd = path.join(root, 'workspace');
  const ctx = { cwd, isProjectTrusted: () => true } as unknown as PiContext;
  const disable = parseMcpManagerAction({ action: 'disable-skill', skill: 'demo-flow', scope: 'project' });
  assert.deepEqual(disable, { action: 'disable-skill', skill: 'demo-flow', scope: 'project' });
  await applyMcpManagerAction(disable, ctx);

  const skillDir = path.join(cwd, '.agents', 'skills', 'demo-flow');
  fs.mkdirSync(skillDir, { recursive: true });
  fs.writeFileSync(path.join(skillDir, 'SKILL.md'), '---\nname: demo-flow\ndescription: Demo workflow.\n---\n');
  const html = await renderMcpManagerPage(ctx);
  assert.match(html, /demo-flow/);
  assert.match(html, /disabled/);
  assert.match(html, /data-action="enable-skill"/);
  assert.match(html, /workspace override/);
  assert.match(html, /"skills"/);
  assert.throws(() => parseMcpManagerAction({ action: 'disable-skill', skill: 'bad\nname' }), /Invalid skill name/);
});

test('settings.html shows live commands plus the complete skill/MCP surface and redacts secret values', async () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'octo-mcp-html-'));
  roots.push(root);
  process.env['OCTOCODE_HOME'] = path.join(root, 'home');
  delete process.env['OCTOCODE_COMPACT_MCP'];
  const cwd = path.join(root, 'workspace');
  const configPath = projectMcpPath(cwd, process.env['OCTOCODE_HOME']);
  fs.mkdirSync(path.dirname(configPath), { recursive: true });
  fs.writeFileSync(configPath, JSON.stringify({ mcpServers: { docs: {
    url: 'https://mcp.example.test/api',
    headers: { Authorization: 'Bearer SECRET', 'X-Tenant': 'acme' },
  } } }));
  const cursorPath = path.join(cwd, '.cursor', 'mcp.json');
  fs.mkdirSync(path.dirname(cursorPath), { recursive: true });
  fs.writeFileSync(cursorPath, JSON.stringify({ mcpServers: {
    browser: { command: 'browser-mcp', args: ['--token=ARG SECRET'], env: { TOKEN: 'FOREIGN SECRET' } },
    remote: { url: 'https://user:pass@mcp.example.test/api?token=QUERY_SECRET#fragment' },
  } }));
  const commands: PiCommand[] = [
    { name: 'configuration', description: 'Open the complete control center.', source: 'extension', sourceInfo: { path: '/extension/index.ts', source: 'octocode', scope: 'temporary', origin: 'package' } },
    { name: 'release', description: 'Run the release workflow.', source: 'skill', sourceInfo: { path: '/skills/release/SKILL.md', source: 'release', scope: 'project', origin: 'top-level' } },
    { name: 'review<script>', description: 'Review & summarize safely.', source: 'prompt', sourceInfo: { path: '/prompts/review.md', source: 'review', scope: 'user', origin: 'top-level' } },
  ];
  const html = await renderMcpManagerPage({ cwd, isProjectTrusted: () => true } as unknown as PiContext, 'test-action-token', undefined, commands);
  assert.equal(SETTINGS_HTML_FILE, 'settings.html');
  assert.match(html, /Octocode · extension control center/);
  assert.match(html, /Your configuration/);
  for (const section of ['overview', 'appearance', 'models', 'hooks', 'plugins', 'diagnostics']) {
    assert.match(html, new RegExp(`id="${section}"`));
    assert.match(html, new RegExp(`href="#${section}"`));
  }
  assert.match(html, /Terminal theme/);
  assert.match(html, /canonical agent-core settings service/);
  assert.match(html, /exact-definition trust/);
  assert.match(html, /transactional contribution registry/);
  assert.match(html, /Runtime controls/);
  assert.match(html, /data-action="set-footer-density"/);
  assert.match(html, /data-action="set-permission-level"/);
  assert.match(html, /3 available now/);
  assert.match(html, /Search commands and descriptions/);
  assert.match(html, /\/configuration/);
  assert.match(html, /Run the release workflow/);
  assert.match(html, /data-command-source="prompt"/);
  assert.match(html, /\/prompts\/review\.md/);
  assert.doesNotMatch(html, /review<script>/);
  assert.match(html, /Search skills/);
  assert.match(html, /Agent prompt catalog/);
  assert.match(html, /Compact mcp\.md guide is injected/);
  assert.match(html, /OCTOCODE_COMPACT_MCP/);
  assert.match(html, /default\/enabled/);
  assert.match(html, /MCP connections/);
  assert.match(html, /--orange:#FF8A3D/);
  assert.match(html, /--violet:#7957D5/);
  assert.match(html, /streamable-http/);
  assert.match(html, /Authorization/);
  assert.doesNotMatch(html, /Bearer SECRET/);
  assert.match(html, /data-action="disable" data-server="docs"/);
  assert.match(html, /Add a managed server/);
  assert.match(html, /Environment references/);
  assert.match(html, /Header references/);
  assert.match(html, /Connect \/ retry/);
  assert.match(html, /Effective scope: project/);
  assert.match(html, /cursor\.browser/);
  assert.match(html, /Discovered from cursor/);
  assert.match(html, /Enable import/);
  assert.match(html, /discovered · read-only · disabled by default/);
  assert.doesNotMatch(html, /FOREIGN SECRET/);
  assert.doesNotMatch(html, /ARG SECRET|QUERY_SECRET|user:pass|fragment/);
  assert.match(html, /Everything lives here/);
  assert.match(html, /run <code>\/settings<\/code>/i);
  assert.match(html, new RegExp(configPath.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')));
  assert.match(html, /x-octocode-action-token/);
  assert.match(html, /test-action-token/);
  assert.doesNotMatch(html, /name="env"|name="headers"/);
});

test('settings.html identifies compact MCP as the enabled default', async () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'octo-mcp-html-compact-'));
  roots.push(root);
  process.env['OCTOCODE_HOME'] = path.join(root, 'home');
  process.env['OCTOCODE_COMPACT_MCP'] = '1';
  const cwd = path.join(root, 'workspace');

  const html = await renderMcpManagerPage({ cwd } as unknown as PiContext);

  assert.match(html, /Compact mcp\.md guide is injected/);
  assert.match(html, /OCTOCODE_COMPACT_MCP/);
  assert.match(html, /enabled/);
});
