import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { afterAll, afterEach, beforeAll, beforeEach, test } from 'vitest';
import { allowLocalFixtureProcesses } from '../../../test-utils/external-effects-guard.js';
import {
  cleanupSpawnedAgentsForShutdown,
  evaluateSpawnPolicy,
  formatAgentLedgerDetails,
  listWorkerLedgerEntries,
  prepareSpawnAgentParams,
  setAgentProcessFactoryForTests,
  spawnRpcAgent,
} from '../src/tools/agent-tools.js';
import { createAgentWorktree, sweepAgentWorktrees } from '../src/tools/worktree.js';
import type { PiContext } from '../src/types.js';
import { makeMockAgentProcess } from './helpers/mock-process.js';

function git(cwd: string, args: string[]): string {
  const result = spawnSync('git', args, { cwd, encoding: 'utf8' });
  if (result.status !== 0) throw new Error(result.stderr || result.stdout || `git ${args.join(' ')} failed`);
  return result.stdout.trim();
}

function tmp(prefix: string): string {
  return fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), prefix)));
}

function initRepo(): string {
  const repo = tmp('octocode-worktree-repo-');
  git(repo, ['init', '--initial-branch=main']);
  git(repo, ['config', 'user.email', 'test@example.com']);
  git(repo, ['config', 'user.name', 'Test User']);
  fs.writeFileSync(path.join(repo, 'README.md'), '# fixture\n', 'utf8');
  git(repo, ['add', 'README.md']);
  git(repo, ['commit', '-m', 'init']);
  return repo;
}

let home: string;
let previousHome: string | undefined;
const cleanupDirs: string[] = [];
let restoreProcessGuard: () => void;

beforeAll(() => {
  restoreProcessGuard = allowLocalFixtureProcesses();
});

afterAll(() => restoreProcessGuard());

beforeEach(() => {
  home = tmp('octocode-worktree-home-');
  cleanupDirs.push(home);
  previousHome = process.env.OCTOCODE_HOME;
  process.env.OCTOCODE_HOME = home;
  setAgentProcessFactoryForTests(null);
});

afterEach(() => {
  cleanupSpawnedAgentsForShutdown();
  setAgentProcessFactoryForTests(null);
  if (previousHome === undefined) delete process.env.OCTOCODE_HOME;
  else process.env.OCTOCODE_HOME = previousHome;
  for (const dir of cleanupDirs.splice(0)) fs.rmSync(dir, { recursive: true, force: true });
});

test('prepareSpawnAgentParams fails closed for non-interactive worktree requests', async () => {
  await assert.rejects(
    () => prepareSpawnAgentParams({ task: 'x', isolation: 'worktree' }, { hasUI: false } as PiContext),
    /requires an interactive UI approval/,
  );
});

test('prepareSpawnAgentParams lets the user choose shared cwd instead of creating a worktree', async () => {
  const ctx = {
    hasUI: true,
    ui: { select: async () => 'Use current repo / shared cwd' },
  } as unknown as PiContext;

  const params = await prepareSpawnAgentParams({ task: 'x', isolation: 'worktree' }, ctx);
  assert.equal(params.isolation, 'shared');
  assert.equal(params.worktreeDecision, 'shared');
});

test('evaluateSpawnPolicy refuses worktree isolation outside a git work tree', () => {
  const dir = tmp('octocode-worktree-not-git-');
  cleanupDirs.push(dir);
  const result = evaluateSpawnPolicy({ task: 'x', cwd: dir, isolation: 'worktree' });
  assert.equal(result.allowed, false);
  assert.match(result.reason ?? '', /git|work tree/i);
});

test('createAgentWorktree creates an isolated branch under Octocode home', () => {
  const repo = initRepo();
  cleanupDirs.push(repo);

  const state = createAgentWorktree({ parentCwd: repo, agentId: '12345678-aaaa-bbbb-cccc-dddddddddddd', name: 'Rex Worker' });

  assert.equal(state.baseCommit, git(repo, ['rev-parse', 'HEAD']));
  assert.match(state.branch, /^octocode\/agents\/rex-worker-12345678$/);
  assert.ok(state.path.startsWith(path.join(home, 'extension', 'worktrees')));
  assert.equal(git(state.path, ['branch', '--show-current']), state.branch);
  assert.equal(fs.existsSync(path.join(state.path, 'README.md')), true);
  if (process.platform !== 'win32') {
    assert.equal(fs.statSync(path.dirname(state.metaPath)).mode & 0o777, 0o700);
    assert.equal(fs.statSync(state.metaPath).mode & 0o777, 0o600);
  }
});

test('spawnRpcAgent runs approved worktree workers in the worktree cwd and cleans no-work exits', () => {
  const repo = initRepo();
  cleanupDirs.push(repo);
  const mock = makeMockAgentProcess();
  let spawnedCwd = '';
  setAgentProcessFactoryForTests((_command, _args, options) => {
    spawnedCwd = options.cwd ?? '';
    return mock as never;
  });

  const record = spawnRpcAgent({ task: 'Goal: g\nContext: c\nScope: s\nOwnership: o\nAcceptance: a\nReturn: r', cwd: repo, isolation: 'worktree', worktreeDecision: 'create' });
  assert.ok(record.worktree, 'record stores worktree metadata');
  assert.equal(spawnedCwd, record.worktree!.path);
  assert.equal(listWorkerLedgerEntries()[0]!.worktree?.branch, record.worktree!.branch);
  assert.match(formatAgentLedgerDetails(), /⎇ agents\//);
  assert.match(String(mock.writes[0]!['message']), /Worktree isolation is active/);

  mock.exitCode = 0;
  mock._emit('close', 0, null);
  assert.equal(fs.existsSync(record.worktree!.path), false, 'clean worktree removed on process close');
});

test('spawnRpcAgent keeps unmerged worktrees and makes records non-prunable', () => {
  const repo = initRepo();
  cleanupDirs.push(repo);
  const mock = makeMockAgentProcess();
  setAgentProcessFactoryForTests(() => mock as never);

  const record = spawnRpcAgent({ task: 'Goal: g\nContext: c\nScope: s\nOwnership: o\nAcceptance: a\nReturn: r', cwd: repo, isolation: 'worktree', worktreeDecision: 'create' });
  fs.writeFileSync(path.join(record.worktree!.path, 'worker.txt'), 'change\n', 'utf8');
  git(record.worktree!.path, ['add', 'worker.txt']);
  git(record.worktree!.path, ['commit', '-m', 'worker change']);

  mock.exitCode = 0;
  mock._emit('close', 0, null);

  assert.equal(fs.existsSync(record.worktree!.path), true, 'worktree with commits is kept');
  assert.equal(listWorkerLedgerEntries().length, 1, 'unmerged worktree record remains visible');
  assert.equal(listWorkerLedgerEntries()[0]!.worktree?.mergeState, 'unmerged');
});

test('sweepAgentWorktrees removes clean orphan metadata but keeps work with unique commits', () => {
  const repo = initRepo();
  cleanupDirs.push(repo);
  const clean = createAgentWorktree({ parentCwd: repo, agentId: 'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa', name: 'Clean' });
  const dirty = createAgentWorktree({ parentCwd: repo, agentId: 'bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb', name: 'Dirty' });
  fs.writeFileSync(path.join(dirty.path, 'worker.txt'), 'change\n', 'utf8');
  git(dirty.path, ['add', 'worker.txt']);
  git(dirty.path, ['commit', '-m', 'worker change']);

  const removed = sweepAgentWorktrees(repo, []);

  assert.equal(removed, 1);
  assert.equal(fs.existsSync(clean.path), false);
  assert.equal(fs.existsSync(dirty.path), true);
});
