import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, writeFileSync, readFileSync, existsSync, rmSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { spawnSync } from 'node:child_process';
import { html } from '../lib/report-html.mjs';

const cli = resolve(import.meta.dirname, '../context-audit.mjs');
const snapshot = () => ({
  workspace: '/fixture', generatedAt: '2026-01-01T00:00:00Z',
  instructions: { files: [], alwaysLoadedTokens: 0 },
  skills: { uniqueCount: 0, descriptionTokens: 0, skills: [] }, mcp: [],
  usage: { days: 1, sessions: 0, calls: 0, servers: {}, shell: {}, tools: {}, errors: {} },
  unusedTools: [], findings: [],
});

test('accepted HTML renders the saved audit without collecting again or changing its evidence', t => {
  const dir = mkdtempSync(join(tmpdir(), 'context-report-'));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const source = join(dir, 'context-audit.json'), marker = join(dir, 'probed');
  const original = JSON.stringify(snapshot());
  writeFileSync(source, original);
  writeFileSync(join(dir, '.mcp.json'), JSON.stringify({ mcpServers: { sentinel: {
    command: process.execPath, args: ['-e', `require('node:fs').writeFileSync(${JSON.stringify(marker)}, 'started')`],
  } } }));
  const result = spawnSync(process.execPath, [cli, '--render', source, '--workspace', dir, '--json'], { cwd: dir, encoding: 'utf8', timeout: 5000 });
  assert.equal(result.status, 0, result.stderr);
  assert.deepEqual(JSON.parse(result.stdout), { output: join(dir, 'context-audit.html'), render: 'unverified' });
  assert.equal(readFileSync(source, 'utf8'), original);
  assert.equal(existsSync(marker), false);
  assert.match(readFileSync(join(dir, 'context-audit.html'), 'utf8'), /Context audit/);
});

test('invalid saved evidence does not replace a previous HTML report', t => {
  const dir = mkdtempSync(join(tmpdir(), 'context-report-'));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const source = join(dir, 'context-audit.json'), output = join(dir, 'context-audit.html');
  writeFileSync(output, 'previous report');
  for (const input of ['{broken', '{}']) {
    writeFileSync(source, input);
    const result = spawnSync(process.execPath, [cli, '--render', source], { encoding: 'utf8', timeout: 5000 });
    assert.notEqual(result.status, 0);
    assert.equal(readFileSync(output, 'utf8'), 'previous report');
  }
  const missing = spawnSync(process.execPath, [cli, '--render', '--json'], { encoding: 'utf8', timeout: 5000 });
  assert.notEqual(missing.status, 0);
  assert.match(missing.stderr, /--render requires a value/);
});

test('saved report values remain escaped text, including severity and numeric-looking fields', () => {
  const report = snapshot(), input = '\"><script>alert(1)</script>';
  report.findings = [{ severity: input, area: input, subject: input, message: input }];
  report.usage.days = input;
  report.usage.tools = { fixture: input };
  report.skills.uniqueCount = input;
  const page = html(report);
  assert.ok(!page.includes('<script>'));
  assert.match(page, /class="sev unknown"/);
  assert.ok(page.includes('&lt;script&gt;'));
});
