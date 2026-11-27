#!/usr/bin/env node
// Run a PR's test at the base revision and at head: a test that passes at base does not cover the change.
import { spawn, spawnSync } from 'node:child_process';
import { closeSync, cpSync, existsSync, mkdirSync, mkdtempSync, openSync, rmSync } from 'node:fs';
import { constants, tmpdir } from 'node:os';
import { dirname, isAbsolute, join, relative, resolve } from 'node:path';
import { parseArgs } from 'node:util';
import { isMainModule } from './lib/entrypoint.mjs';

const HELP = `Usage: node scripts/test-at-base.mjs --base <rev> --test <path> [--test <path> ...] --cmd "<command>" [options]

Runs <command> in a temporary git worktree at <rev> with each --test copied in from the checkout,
then in the checkout itself (head). A test that passes at base does not cover the change.

Required:
  --base <rev>         Base revision: the PR's baseCommitSha, or the merge base of a prepared branch.
  --test <path>        Test file or directory to carry from the checkout into the base tree; repeatable.
  --cmd "<command>"    Shell command that runs the test(s), in the same subdirectory of each tree.
Options:
  --setup "<command>"  Shell command run in the base tree before --cmd (install dependencies, build).
  --head <rev>         Refuse unless the checkout's HEAD is this revision (the pinned head).
  --repo <dir>         Directory inside the checkout to run from (default: the current directory).
  --logs <dir>         Directory for setup.log, base.log and head.log (default: a new folder per run under
                       <checkout>/.octocode/tmp/octocode-pr-review/test-at-base/, kept for reading).
  --help               Show this help.

Effects: adds a detached worktree under the system temporary directory (registered in the checkout's
.git/worktrees, checkout hooks disabled) and removes it on success, failure or interrupt. The script
writes only its logs (by default under the checkout's .octocode/tmp/) and leaves the checkout's other files
untouched; --cmd runs there as given.

Output (stdout, one JSON line):
  {"ok":true,"covers":<bool>,"tests":[...],"logs":"<dir>",
   "base":{"rev","exit","signal","log"},"head":{"rev","dirty","exit","signal","log"},"setup"?:{"exit","signal","log"}}
  covers is true only when base fails and head passes. Read base.log before citing it: a base failure
  from missing setup, rather than from the test's assertions, is not coverage.
Exit codes: 0 both runs finished (read covers); 1 failed run {"ok":false,"error":{"code","message"}}
  (worktree, setup or command start); 2 invalid input (arguments, repository, revision, test path).`;

class Failure extends Error {
  constructor(code, message, exitCode = 1, extra = {}) {
    super(message);
    this.code = code;
    this.exitCode = exitCode;
    this.extra = extra;
  }
}

const invalid = (code, message) => new Failure(code, message, 2);

function git(cwd, args) {
  const result = spawnSync('git', args, { cwd, encoding: 'utf8' });
  if (result.error) throw new Failure('gitUnavailable', result.error.message);
  return { ok: result.status === 0, out: result.stdout.trim(), err: result.stderr.trim() };
}

function revision(top, rev) {
  const result = git(top, ['rev-parse', '--verify', '--quiet', `${rev}^{commit}`]);
  if (!result.ok) throw invalid('unknownRevision', `${rev} is not a commit in ${top}`);
  return result.out;
}

export function parse(argv) {
  let values;
  try {
    ({ values } = parseArgs({
      args: argv,
      strict: true,
      options: {
        base: { type: 'string' }, test: { type: 'string', multiple: true }, cmd: { type: 'string' },
        setup: { type: 'string' }, head: { type: 'string' }, repo: { type: 'string' },
        logs: { type: 'string' }, help: { type: 'boolean' },
      },
    }));
  } catch (error) {
    throw invalid('invalidArguments', error.message);
  }
  if (values.help) return { help: true };
  for (const name of ['base', 'cmd']) if (!values[name]?.trim()) throw invalid('invalidArguments', `--${name} is required`);
  if (!values.test?.length) throw invalid('invalidArguments', '--test is required');
  return values;
}

// Runs a shell command with its output in one log file; `state.child` lets an interrupt stop it.
function run(command, cwd, log, state) {
  const fd = openSync(log, 'w');
  return new Promise((done, reject) => {
    const child = spawn(command, { cwd, shell: true, stdio: ['ignore', fd, fd], detached: process.platform !== 'win32' });
    state.child = child;
    child.on('error', error => {
      state.child = null;
      closeSync(fd);
      reject(new Failure('commandNotStarted', `${command}: ${error.message}`));
    });
    child.on('close', (exit, signal) => {
      state.child = null;
      closeSync(fd);
      done({ exit, signal, log });
    });
  });
}

const passed = result => result.exit === 0;

export async function testAtBase(options, state = {}) {
  const repoDir = resolve(options.repo ?? '.');
  if (!existsSync(repoDir)) throw invalid('notGitRepository', `${repoDir} does not exist`);
  const topResult = git(repoDir, ['rev-parse', '--show-toplevel']);
  if (!topResult.ok) throw invalid('notGitRepository', `${repoDir} is not inside a git checkout`);
  const top = topResult.out;
  const prefix = git(repoDir, ['rev-parse', '--show-prefix']).out;
  const baseRev = revision(top, options.base);
  const headRev = revision(top, 'HEAD');
  if (options.head && revision(top, options.head) !== headRev) {
    throw invalid('headMismatch', `checkout HEAD is ${headRev}, not the pinned head ${options.head}`);
  }
  const tests = options.test.map(path => {
    const full = resolve(repoDir, path);
    const rel = relative(top, full);
    if (!rel || rel.startsWith('..') || isAbsolute(rel)) throw invalid('testOutsideRepository', `${path} is not inside ${top}`);
    if (!existsSync(full)) throw invalid('testMissing', `${path} does not exist in the checkout`);
    return rel;
  });
  const logRoot = join(top, '.octocode', 'tmp', 'octocode-pr-review', 'test-at-base');
  if (!options.logs) mkdirSync(logRoot, { recursive: true });
  // Each default run gets its own folder, kept so its result can be read.
  const logs = options.logs ? resolve(options.logs) : mkdtempSync(join(logRoot, `${new Date().toISOString().replace(/[:.]/g, '-')}-`));
  mkdirSync(logs, { recursive: true });
  // Logs kept inside the checkout (this run's, and every earlier run's default folder) are this script's output,
  // not uncommitted work.
  const own = [logs, logRoot].map(dir => relative(top, dir)).filter(rel => rel && !rel.startsWith('..') && !isAbsolute(rel));
  const ownLogs = own.length ? ['--', '.', ...own.map(rel => `:(exclude)${rel}`)] : [];
  const dirty = git(top, ['status', '--porcelain', ...ownLogs]).out !== '';

  const scratch = mkdtempSync(join(tmpdir(), 'test-at-base-'));
  const hooks = join(scratch, 'hooks');
  const tree = join(scratch, 'base');
  mkdirSync(hooks);
  state.cleanup = () => {
    state.cleanup = null;
    if (existsSync(tree) && !git(top, ['worktree', 'remove', '--force', tree]).ok) {
      rmSync(tree, { recursive: true, force: true });
      git(top, ['worktree', 'prune']);
    }
    rmSync(scratch, { recursive: true, force: true });
  };
  try {
    const added = git(top, ['-c', `core.hooksPath=${hooks}`, 'worktree', 'add', '--detach', tree, baseRev]);
    if (!added.ok) throw new Failure('worktreeFailed', added.err || `git worktree add failed at ${baseRev}`);
    for (const rel of tests) {
      mkdirSync(dirname(join(tree, rel)), { recursive: true });
      cpSync(join(top, rel), join(tree, rel), { recursive: true, force: true });
    }
    const baseCwd = join(tree, prefix);
    mkdirSync(baseCwd, { recursive: true });
    const result = { ok: true, covers: false, tests, logs };
    if (options.setup) {
      result.setup = await run(options.setup, baseCwd, join(logs, 'setup.log'), state);
      if (!passed(result.setup)) throw new Failure('setupFailed', `--setup failed at base; read ${result.setup.log}`, 1, { setup: result.setup });
    }
    result.base = { rev: baseRev, ...await run(options.cmd, baseCwd, join(logs, 'base.log'), state) };
    result.head = { rev: headRev, dirty, ...await run(options.cmd, join(top, prefix), join(logs, 'head.log'), state) };
    result.covers = !passed(result.base) && passed(result.head);
    return result;
  } finally {
    state.cleanup?.();
  }
}

function emitFailure(error) {
  const failure = error instanceof Failure ? error : new Failure('runFailed', error instanceof Error ? error.message : String(error));
  console.log(JSON.stringify({ ok: false, ...failure.extra, error: { code: failure.code, message: failure.message } }));
  process.exitCode = failure.exitCode;
}

async function main(argv) {
  const state = {};
  for (const signal of ['SIGINT', 'SIGTERM', 'SIGHUP']) {
    process.on(signal, () => {
      const child = state.child;
      if (child?.pid) {
        try { process.kill(process.platform === 'win32' ? child.pid : -child.pid, signal); } catch { /* already gone */ }
      }
      state.cleanup?.();
      console.log(JSON.stringify({ ok: false, error: { code: 'interrupted', message: `stopped by ${signal}; worktree removed` } }));
      process.exit(128 + constants.signals[signal]);
    });
  }
  try {
    const options = parse(argv);
    if (options.help) {
      console.log(HELP);
      return;
    }
    console.log(JSON.stringify(await testAtBase(options, state)));
  } catch (error) {
    emitFailure(error);
  }
}

if (isMainModule(import.meta.url)) await main(process.argv.slice(2));
