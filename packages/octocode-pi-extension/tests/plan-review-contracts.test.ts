import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { test } from 'vitest';

const packageRoot = path.resolve(import.meta.dirname, '..');
const fixtureDir = path.join(packageRoot, 'tests', 'fixtures');

function readJson(name: string): Record<string, unknown> {
  return JSON.parse(fs.readFileSync(path.join(fixtureDir, name), 'utf8')) as Record<string, unknown>;
}

test('browser protocol fixture freezes the reviewed security contract', () => {
  const fixture = readJson('plan-review-browser-protocol-v1.json');
  assert.equal(fixture['version'], 1);

  const capability = fixture['capability'] as Record<string, unknown>;
  assert.equal(capability['randomBytes'], 32);
  assert.equal(capability['entropyBits'], 256);
  assert.equal(capability['persisted'], false);

  const requestPolicy = fixture['requestPolicy'] as Record<string, unknown>;
  assert.equal(requestPolicy['exactHost'], '127.0.0.1:<port>');
  assert.equal(requestPolicy['postRejectMissingOrNullOrigin'], true);
  assert.equal(requestPolicy['getOrigin'], 'optional-but-exact-when-present');
  assert.equal(requestPolicy['corsResponseHeader'], false);
  assert.equal(requestPolicy['optionsAllowed'], false);

  const limits = fixture['limits'] as Record<string, unknown>;
  assert.equal(limits['requestBodyBytes'], 64 * 1024);
  assert.equal(limits['answerOrCommentBytes'], 8 * 1024);
  assert.equal(limits['acceptsFilesystemPath'], false);
  assert.equal(limits['acceptsCommand'], false);

  const idempotency = fixture['idempotency'] as Record<string, unknown>;
  assert.equal(idempotency['operationIdBits'], 128);
  assert.equal(idempotency['duplicateBehavior'], 'return-original-response-without-append-or-state-change');

  const csp = String(fixture['csp']);
  for (const directive of ["default-src 'none'", "script-src 'self'", "connect-src 'self'", "frame-ancestors 'none'"]) {
    assert.ok(csp.includes(directive), `missing CSP directive: ${directive}`);
  }
});

test('UX corpus and rubric freeze reproducible launch evidence', () => {
  const fixture = readJson('plan-review-ux-v1.json');
  assert.equal(fixture['version'], 1);
  const tasks = fixture['tasks'] as Array<Record<string, unknown>>;
  assert.equal(tasks.length, 10);
  assert.equal(new Set(tasks.map((task) => task['id'])).size, 10);

  const categories = new Set(tasks.map((task) => task['category']));
  for (const category of ['ordinary-question', 'comparison-question', 'rfc-review', 'revision-flow', 'acceptance-flow', 'safety-flow']) {
    assert.ok(categories.has(category), `missing UX category: ${category}`);
  }
  assert.ok(tasks.some((task) => task['terminalProfile'] === '44-column-color'));
  assert.ok(tasks.some((task) => task['terminalProfile'] === '80-column-no-color'));
  assert.ok(tasks.some((task) => task['prohibited'] === 'implementation-start'));
  assert.ok(tasks.some((task) => task['prohibited'] === 'workspace-mutation'));

  const density = fixture['defaultDensityFixture'] as Record<string, unknown>;
  assert.equal(density['terminalColumns'], 80);
  assert.equal(density['optionCount'], 3);
  assert.equal(density['baselineVisibleLines'], 11);
  assert.equal(density['targetMaximumVisibleLines'], 6);
  assert.ok(Number(density['targetMaximumVisibleLines']) <= Number(density['baselineVisibleLines']) * 0.6);

  const rubric = fs.readFileSync(path.join(fixtureDir, 'plan-review-ux-rubric-v1.md'), 'utf8');
  for (const required of ['five participants', 'all ten tasks', '45/50', 'zero accidental Starts', 'at most 60%', 'gates default rollout']) {
    assert.ok(rubric.includes(required), `rubric is missing: ${required}`);
  }
});

test('session artifact producer inventory closes ownership and storage decisions', () => {
  const fixture = readJson('session-artifact-producers-v1.json');
  assert.equal(fixture['version'], 1);

  const identity = fixture['identityDecision'] as Record<string, unknown>;
  assert.deepEqual(identity['precedence'], ['session-manager-id', 'resolved-session-file', 'process-workspace-fallback']);
  assert.equal(identity['workspaceIsolation'], true);
  assert.equal(identity['rawSessionFilePersisted'], false);

  const storage = fixture['planStorageDecision'] as Record<string, unknown>;
  assert.equal(storage['customEntryAuthority'], 'active-branch-only');
  assert.equal(storage['projection'], 'branch-snapshot-plus-generation-CAS');

  const producers = fixture['producers'] as Array<Record<string, unknown>>;
  assert.equal(new Set(producers.map((producer) => producer['id'])).size, producers.length);
  assert.ok(producers.length >= 16, 'repository-wide producer inventory remains exhaustive');
  assert.ok(producers.some((producer) => producer['id'] === 'chrome-connection-state' && producer['classification'] === 'global-reusable-browser-connection'));
  assert.ok(producers.some((producer) => producer['id'] === 'workspace-error-log' && producer['classification'] === 'workspace-shared-diagnostic'));
  assert.ok(producers.some((producer) => producer['id'] === 'pi-session-transcript' && producer['classification'] === 'external-owner'));

  for (const producer of producers) {
    assert.equal(typeof producer['id'], 'string');
    assert.equal(typeof producer['source'], 'string');
    assert.equal(typeof producer['symbol'], 'string');
    assert.ok(['current', 'migrated', 'pending-session-migration', 'explicit-exclusion'].includes(String(producer['status'])));
  }
});

test('new session-owned roots are centralized or explicitly frozen as migration debt', () => {
  const srcRoot = path.join(packageRoot, 'src');
  const allowedDebt = new Set([
    'tools/compaction-artifacts.ts',
    'tools/plan-html.ts',
  ]);
  const owner = 'tools/session-artifacts.ts';
  const rootConstructor = /getOctocodeHome\(\)[\s\S]{0,180}(?:'plans'|'tmp'\s*,\s*'plan'|'tmp'\s*,\s*'compaction')/m;
  const violations: string[] = [];

  for (const entry of fs.readdirSync(path.join(srcRoot, 'tools'), { withFileTypes: true })) {
    if (!entry.isFile() || !entry.name.endsWith('.ts')) continue;
    const relative = `tools/${entry.name}`;
    if (relative === owner || allowedDebt.has(relative)) continue;
    const source = fs.readFileSync(path.join(srcRoot, relative), 'utf8');
    if (rootConstructor.test(source)) violations.push(relative);
  }

  assert.deepEqual(violations, [], `new direct session-owned roots must route through ${owner}: ${violations.join(', ')}`);
});
