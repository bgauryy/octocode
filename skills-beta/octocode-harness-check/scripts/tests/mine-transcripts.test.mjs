import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, realpathSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { after, before, test } from 'node:test';

const script = fileURLToPath(new URL('../mine-transcripts.mjs', import.meta.url));
// Claude Code names ~/.claude/projects/<slug> after the physical launch path.
const slugOf = (path) => path.replace(/[^A-Za-z0-9]/g, '-');

let base;
let home;
let repo;
let sibling;

// One session file: `calls` tool uses recorded at `cwd` (default: the launch path).
function session(launch, name, calls, cwd = launch) {
  const dir = join(home, '.claude', 'projects', slugOf(launch));
  mkdirSync(dir, { recursive: true });
  const record = (type, content, at = cwd) => JSON.stringify({ type, cwd: at, timestamp: '2026-09-20T10:00:00.000Z', message: { content } });
  const lines = [record('user', 'start', launch)];
  calls.forEach((tool, i) => {
    const id = `toolu_${name}_${i}`;
    lines.push(record('assistant', [{ type: 'tool_use', id, name: `mcp__octocode-local__${tool}`, input: { path: '.' } }]));
    lines.push(record('user', [{ type: 'tool_result', tool_use_id: id, content: [{ type: 'text', text: '{"results":[]}' }] }]));
  });
  writeFileSync(join(dir, `${name}.jsonl`), `${lines.join('\n')}\n`);
}

before(() => {
  base = realpathSync(mkdtempSync(join(tmpdir(), 'mine-transcripts-')));
  home = join(base, 'home');
  repo = join(base, 'code', 'octocode');
  sibling = join(base, 'code', 'octocode-mcp-host');
  mkdirSync(join(repo, 'packages', 'native'), { recursive: true });
  mkdirSync(sibling, { recursive: true });
  mkdirSync(join(base, 'empty'));
  symlinkSync(repo, join(base, 'repo-link'), 'junction');
  session(repo, 'root', ['localSearch', 'localFetch']);
  session(join(repo, 'packages', 'native'), 'subdir', ['astSearch']);
  // Same `<slug>-` prefix as repo; its later records even cd into repo.
  session(sibling, 'sibling', ['ghSearch', 'ghSearch', 'ghSearch'], repo);
});
after(() => rmSync(base, { recursive: true, force: true }));

function mine(args, cwd = repo) {
  const result = spawnSync(process.execPath, [script, ...args], {
    cwd,
    encoding: 'utf8',
    env: { ...process.env, HOME: home, USERPROFILE: home },
  });
  assert.equal(result.status, 0, result.stderr);
  return result;
}
const json = (args, cwd) => JSON.parse(mine(args, cwd).stdout);
const callsByTool = (out) => Object.fromEntries(out.tools.map((t) => [t.tool, t.calls]));

test('documented --cwd . reads the repository sessions, not a false zero', () => {
  const out = json(['--cwd', '.']);
  assert.equal(out.cwd, repo);
  assert.equal(out.sessionDirs, 2);
  assert.equal(out.calls, 3);
  assert.deepEqual(callsByTool(out), { localSearch: 1, localFetch: 1, astSearch: 1 });
  assert.match(mine(['--cwd', '.', '--table']).stdout, /^sessions dirs: 2 {2}calls: 3$/m);
});

test('relative, trailing-slash, and symlinked --cwd spellings match the absolute path', () => {
  const expected = json(['--cwd', repo]);
  assert.equal(expected.calls, 3);
  for (const [args, cwd] of [
    [[], repo],
    [['--cwd', `${repo}/`], base],
    [['--cwd', '../..'], join(repo, 'packages', 'native')],
    [['--cwd', 'code/octocode'], base],
    [['--cwd', join(base, 'repo-link')], base],
  ]) {
    assert.deepEqual(json(args, cwd), expected, `mine-transcripts ${args.join(' ')} from ${cwd}`);
  }
});

test('a sibling repository that shares the slug prefix is not counted as a subdirectory', () => {
  assert.equal(json(['--cwd', repo]).tools.some((t) => t.tool === 'ghSearch'), false);
  assert.deepEqual(callsByTool(json(['--cwd', sibling], base)), { ghSearch: 3 });
});

test('a directory with no recorded sessions reports zero and says why on stderr', () => {
  const empty = join(base, 'empty');
  const result = mine(['--cwd', '.'], empty);
  assert.equal(JSON.parse(result.stdout).calls, 0);
  assert.ok(result.stderr.includes(`no Claude Code sessions recorded for ${empty}`), result.stderr);
});
