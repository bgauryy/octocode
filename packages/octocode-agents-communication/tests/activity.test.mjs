import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from './helpers.mjs';
import { existsSync, mkdirSync, rmSync, writeFileSync, renameSync, utimesSync } from 'node:fs';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { registerBoundTools } from '../scripts/pi-extension.mjs';
import { nativeBinary as binary, tempWorkspace } from './helpers.mjs';

function fixture(t) {
  const workspace = tempWorkspace(t, 'communication-activity-');
  const database = join(workspace, 'absent/store.sqlite');
  const git = (args, extra = {}) => execFileSync('git', args, { cwd: workspace, encoding: 'utf8', stdio: ['pipe', 'pipe', 'pipe'], ...extra }).trim();
  git(['init', '-q']);
  const put = (path, text = path, at) => { mkdirSync(join(workspace, path, '..'), { recursive: true }); writeFileSync(join(workspace, path), text); if (at) utimesSync(join(workspace, path), at / 1000, at / 1000); };
  // Build history objects in this disposable fixture; never commit the shared checkout.
  let parent;
  const history = (subject, at) => {
    git(['add', '-A']);
    const tree = git(['write-tree']);
    const env = { ...process.env, GIT_AUTHOR_NAME: 'Fixture', GIT_AUTHOR_EMAIL: 'fixture@example.invalid', GIT_COMMITTER_NAME: 'Fixture', GIT_COMMITTER_EMAIL: 'fixture@example.invalid', GIT_AUTHOR_DATE: `${at} +0000`, GIT_COMMITTER_DATE: `${at} +0000` };
    parent = git(['commit-tree', tree, ...(parent ? ['-p', parent] : []), '-m', subject], { env });
    git(['update-ref', '--create-reflog', '-m', subject, 'HEAD', parent], { env });
    return parent;
  };
  const invoke = (command, input = {}, scope = workspace) => JSON.parse(execFileSync(binary, [command, JSON.stringify(input), '--workspace', scope, '--database', database], { encoding: 'utf8', stdio: ['pipe', 'pipe', 'pipe'] }));
  return { workspace, database, git, put, history, invoke, activity: (input = {}, scope) => invoke('activity', input, scope) };
}

test('activity handles unborn history and does not create coordination storage', t => {
  const f = fixture(t);
  f.put('new.txt');
  assert.equal(f.activity().items[0].path, 'new.txt');
  for (const view of ['commits', 'reflog']) assert.deepEqual(f.activity({ view }).items, []);
  assert.equal(existsSync(f.database), false);
});

test('dirty paths filter by time, literal prefix and regex, preserving unusual names and deletion uncertainty', t => {
  const f = fixture(t), base = 1700000000000;
  for (const path of ['src/a.rs', 'src/deleted.rs', 'src/rename.rs', 'outside.rs']) f.put(path);
  f.history('initial', base / 1000);
  f.put('src/a.rs', 'changed', base + 1000);
  f.put('src/new space\n日本.rs', 'new', base + 2000);
  f.put('outside.rs', 'changed', base + 3000);
  rmSync(join(f.workspace, 'src/deleted.rs'));
  renameSync(join(f.workspace, 'src/rename.rs'), join(f.workspace, 'src/renamed.rs'));
  f.git(['add', 'src/rename.rs', 'src/renamed.rs']);
  utimesSync(join(f.workspace, 'src/renamed.rs'), base / 1000, base / 1000);
  const rows = f.activity({ path: 'src/', pathRegex: '\\.rs$', sinceMs: base + 1000, untilMs: base + 2000 });
  assert.deepEqual(rows.items.map(r => r.path), ['src/new space\n日本.rs', 'src/a.rs']);
  assert.equal(rows.coverage.unknownFileTimes, 1);
  assert.deepEqual(rows.items[0].changes, ['untracked']);
  const recovered=f.activity(rows.coverage.unknownTimeQuery);
  const deleted=recovered.items.find(row=>row.path==='src/deleted.rs');
  assert.deepEqual(deleted.changes,['deleted']);
  assert.equal(deleted.indexState,'unchanged');
  assert.equal(deleted.worktreeState,'deleted');
  assert.equal(deleted.modifiedAt,undefined);
  assert.equal(rows.coverage.unknownTimeQuery.sinceMs,undefined);
  assert.equal(rows.coverage.unknownTimeQuery.untilMs,undefined);
  assert.equal(rows.coverage.unknownTimeQuery.path,'src/');
  assert.equal(rows.coverage.scanned, 5);
  const renamed = f.activity({ path: 'src/rename.rs' }).items[0];
  assert.deepEqual(renamed.changes,['renamed']);
  assert.equal(renamed.path, 'src/renamed.rs'); assert.equal(renamed.previousPath, 'src/rename.rs');
  assert.equal(f.activity({ path: 'src/deleted.rs' }).items[0].modifiedAt, undefined);
  assert.equal(f.activity({ path: 'sr' }).items.length, 0);
  assert.deepEqual(f.activity({ path: 'a.rs' }, join(f.workspace, 'src')).items.map(r => r.path), ['a.rs']);
});

test('activity continuation freezes relative time and rejects changed snapshots', t => {
  const f = fixture(t);
  for (const name of ['a', 'b', 'c']) f.put(name);
  const first = f.activity({ limit: 1, withinMs: 60000 });
  assert.equal(first.items.length, 1); assert.equal(first.totalMatched, 3);
  assert.equal(first.next.input.withinMs, undefined); assert.equal(typeof first.next.input.sinceMs, 'number');
  const second = f.activity(first.next.input), third = f.activity(second.next.input);
  assert.equal(new Set([...first.items, ...second.items, ...third.items].map(r => r.path)).size, 3);
  assert.equal(third.next, undefined);
  f.put('extra');
  assert.throws(() => f.activity(first.next.input), /Activity changed during pagination/);
});

test('commit and reflog views distinguish timestamps, bound coverage and filter workspace paths', t => {
  const f = fixture(t), at = 1700000000;
  f.put('src/a.rs'); const first = f.history('initial', at);
  f.put('docs/a.md'); f.history('docs update', at + 10);
  f.put('src/a.rs', 'second'); const last = f.history('source update', at + 20);
  const recent = f.activity({ view: 'commits', path: 'src', pathRegex: '\\.rs$', sinceMs: (at + 1) * 1000 });
  assert.deepEqual(recent.items.map(r => r.hash), [last]);
  assert.deepEqual(recent.items[0].paths, ['src/a.rs']);
  assert.equal(recent.items[0].at, (at + 20) * 1000);
  const bounded = f.activity({ view: 'commits', scanLimit: 1, untilMs: at * 1000 });
  assert.equal(bounded.items.length, 0); assert.equal(bounded.coverage.scanned, 1); assert.equal(bounded.coverage.truncated, true);
  assert.deepEqual(f.activity({ view: 'commits', path: 'src', untilMs: at * 1000 }).items.map(r => r.hash), [first]);
  assert.deepEqual(f.activity({ view: 'commits' }, join(f.workspace, 'src')).items[0].paths, ['a.rs']);
  const reflog = f.activity({ view: 'reflog', sinceMs: (at + 15) * 1000 });
  assert.equal(reflog.items.length, 1); assert.equal(reflog.items[0].action, 'source update');
  assert.match(reflog.timeBasis, /not complete Git command history/);
  assert.equal(existsSync(f.database), false);
});

test('activity rejects ambiguous or unsafe filters and advertises the bound read-only contract', t => {
  const f = fixture(t);
  for (const input of [{ path: '/' }, { path: '../src' }, { path: 'src/../x' }, { path: './src' }, { pathRegex: '(' }, { pathRegex: '(?=src)' }, { sinceMs: 10, untilMs: 1 }, { sinceMs: 1, withinMs: 1 }, { view: 'reflog', path: 'src' }, { after: 1 }, { limit: 101 }]) assert.throws(() => f.activity(input), JSON.stringify(input));
  const catalog = JSON.parse(execFileSync(binary, ['schema'], { encoding: 'utf8' }));
  const tool = catalog.tools.find(t => t.name === 'activity');
  assert.equal(tool.annotations.readOnlyHint, true);
  assert.deepEqual(tool.inputSchema, catalog.commands.find(c => c.name === 'activity').inputSchema);
});

test('history preserves empty commits and newline paths; samples large path lists explicitly', t => {
  const f = fixture(t), at = 1700000000;
  f.put('\nleading.txt');
  f.history('first', at);
  f.history('empty', at + 1);
  for (let i = 0; i < 25; i++) f.put(`bulk/${i}.rs`);
  f.history('x'.repeat(300), at + 2);
  const rows = f.activity({ view: 'commits' }).items;
  assert.equal(rows.length, 3);
  assert.equal(rows[0].paths.length, 20); assert.equal(rows[0].matchedPathCount, 25); assert.equal(rows[0].pathsTruncated, true);
  assert.equal(rows[0].subject.length, 256); assert.equal(rows[0].subjectTruncated, true);
  assert.equal(rows[1].subject, 'empty'); assert.deepEqual(rows[1].paths, []);
  assert.deepEqual(rows[2].paths, ['\nleading.txt']);
});

test('workspace subdirectory does not lose moves across its boundary', t => {
  const f = fixture(t);
  f.put('src/outbound.rs'); f.put('outside/inbound.rs'); f.history('initial', 1700000000);
  renameSync(join(f.workspace, 'src/outbound.rs'), join(f.workspace, 'outside/outbound.rs'));
  renameSync(join(f.workspace, 'outside/inbound.rs'), join(f.workspace, 'src/inbound.rs'));
  f.git(['add', '-A']);
  const rows = f.activity({}, join(f.workspace, 'src')).items;
  assert.deepEqual(rows.map(r => r.path).sort(), ['inbound.rs', 'outbound.rs']);
});

test('bound MCP activity observes its registered workspace without mutating audit', t => {
  const f = fixture(t); f.put('one.txt');
  const session = f.invoke('join', { name: 'observer', vendor: 'generic' });
  const db = new DatabaseSync(f.database, { readOnly: true });
  t.after(() => db.close());
  const before = db.prepare('SELECT count(*) AS count FROM records').get().count;
  const frames = [{ jsonrpc: '2.0', id: 1, method: 'tools/call', params: { name: 'activity', arguments: { limit: 1, path: 'one.txt' } } }].map(JSON.stringify).join('\n') + '\n';
  const result = JSON.parse(execFileSync(binary, ['mcp', '--workspace', f.workspace, '--database', f.database, '--session', session.id], { encoding: 'utf8', input: frames }));
  assert.equal(result.result.isError, undefined);
  const observation = JSON.parse(result.result.content[0].text);
  assert.equal(observation.items[0].path, 'one.txt');
  assert.equal(observation.view, 'files');
  assert.equal(db.prepare('SELECT count(*) AS count FROM records').get().count, before);
});

test('Pi bound activity uses the same schema and executable', async t => {
  const f = fixture(t); f.put('pi.txt');
  const session = f.invoke('join', { name: 'pi-observer', vendor: 'pi' });
  const catalog = JSON.parse(execFileSync(binary, ['schema'], { encoding: 'utf8' }));
  const registered = new Map();
  registerBoundTools({ registerTool: tool => registered.set(tool.name, tool) }, { binary, workspace: f.workspace, database: f.database, session: session.id, tools: catalog.tools });
  const result = await registered.get('activity').execute('observe', { path: 'pi.txt' });
  assert.equal(JSON.parse(result.content[0].text).items[0].path, 'pi.txt');
});
