/**
 * Bounded Git observations, never an attribution or complete command audit.
 *
 * `pathRegex` dialect: patterns match with JavaScript's RegExp and the `u` flag, so look-around and backreferences are refused,
 * and: `\d \w \s` and `\b` are ASCII-only
 * (`\s` follows the ECMAScript whitespace set), no `(?P<name>...)` (use `(?<name>...)`), no `\A`/`\Z` (use `^`/`$`; `$` matches only at
 * the end of the path), no inline flags such as `(?i)`, and the `u` flag rejects identity escapes such as `\-` outside
 * character classes. Matching is cut off at the request's shared 5-second deadline, like Git itself.
 */
import { createHash } from 'node:crypto';
import { spawn } from 'node:child_process';
import { lstatSync } from 'node:fs';
import { join } from 'node:path';
import { runInNewContext } from 'node:vm';
import { PAGE_BYTES } from './catalog.js';
import type { ActivityInput, CommandResults } from './catalog-types.generated.js';
import { ValidationError } from './errors.js';
import { realpath } from './paths.js';
import { isWithin, relativePath } from './store-types.js';
import { codePointLength, compareCodePoints, truncateCodePoints } from './text.js';
import { monotonic, now } from './timing.js';
import { decodeUtf8, encode } from './json-text.js';

type Item = CommandResults['activity']['items'][number];
type FileItem = Extract<Item, { indexState: string }>;
/** A history row with the temporary flag that tells whether it passes the time and path filters. */
type Dated = (Extract<Item, { subject: string }> | Extract<Item, { action: string }>) & { matches: boolean };

const MAX_BYTES = 8 * 1024 * 1024;
const DEADLINE_MS = 5000;
const TEXT_LIMIT = 256;
const FRAGMENT_LIMIT = 20;
const UNSUPPORTED = /\(\?(?:[=!]|<[=!]|P=|\()|\\[1-9]|\\[gk][<{]/;
const GIT_SCOPE = ['GIT_DIR', 'GIT_WORK_TREE', 'GIT_INDEX_FILE', 'GIT_COMMON_DIR', 'GIT_OBJECT_DIRECTORY', 'GIT_ALTERNATE_OBJECT_DIRECTORIES', 'GIT_CONFIG_COUNT', 'GIT_CONFIG_PARAMETERS'];

const EXCEEDED = 'Git activity exceeded 5 seconds; use a smaller workspace for files or lower scanLimit for history';

class Git {
  private readonly deadline = monotonic() + DEADLINE_MS;

  /** Milliseconds left before the shared deadline, at least 1. */
  remaining(): number {
    return Math.max(1, Math.floor(this.deadline - monotonic()));
  }

  execute(args: readonly string[], cwd: string, env: NodeJS.ProcessEnv): Promise<string> {
    return new Promise((resolve, reject) => {
      const child = spawn('git', args, { cwd, env, stdio: ['ignore', 'pipe', 'pipe'], detached: process.platform !== 'win32' });
      const out: Buffer[] = [];
      const err: Buffer[] = [];
      const size = { out: 0, err: 0 };
      let settled = false;
      const finish = (failure: Error | null, value = ''): void => {
        if (settled) return;
        settled = true;
        clearTimeout(timer);
        if (child.exitCode === null && child.signalCode === null) {
          try {
            if (process.platform !== 'win32') process.kill(-child.pid!, 'SIGKILL');
          } catch {
            /* the group is already gone */
          }
          child.kill('SIGKILL');
        }
        if (failure) reject(failure);
        else resolve(value);
      };
      const timer = setTimeout(() => finish(new ValidationError(EXCEEDED)), Math.max(0, this.deadline - monotonic()));
      const collect = (chunks: Buffer[], key: 'out' | 'err') => (chunk: Buffer): void => {
        size[key] += chunk.length;
        if (size[key] > MAX_BYTES) finish(new ValidationError('Git activity exceeds 8 MiB; use a smaller workspace for files or lower scanLimit for history'));
        else chunks.push(chunk);
      };
      child.stdout.on('data', collect(out, 'out'));
      child.stderr.on('data', collect(err, 'err'));
      child.on('error', error => finish((error as NodeJS.ErrnoException).code === 'ENOENT' ? new ValidationError('Git is required for activity') : error));
      child.on('close', (status, signal) => {
        if (settled) return;
        try {
          const result = decodeUtf8(Buffer.concat(out));
          // A child killed by a signal has no exit status (null) and its output may be cut short: a failure, never a result.
          if (status !== 0 || signal !== null) finish(new ValidationError('Git activity failed: ' + truncateCodePoints(Buffer.concat(err).toString('utf8'), 2000)));
          else finish(null, result);
        } catch (error) {
          finish(error as Error);
        }
      });
    });
  }

  read(cwd: string, args: readonly string[]): Promise<string> {
    const env = { ...process.env };
    for (const key of GIT_SCOPE) delete env[key];
    env.GIT_NO_LAZY_FETCH = '1';
    return this.execute(['--no-pager', '--no-optional-locks', '-c', 'core.fsmonitor=false', '-c', 'core.untrackedCache=false', '-c', 'log.showSignature=false', ...args], cwd, env);
  }
}

class Filter {
  readonly prefix: string;
  readonly pattern: string | undefined;
  readonly since: number | undefined;
  readonly until: number | undefined;
  private allowed: Set<string> | null = null;

  constructor(input: ActivityInput, at: number) {
    const prefix = input.path ?? '';
    if (prefix.startsWith('/') || prefix.split('/').some(part => part === '.' || part === '..') || prefix.includes('\\')) throw new ValidationError('path must be a workspace-relative slash-separated prefix without . or ..');
    this.prefix = prefix.replace(/\/+$/, '');
    this.pattern = input.pathRegex;
    if (this.pattern !== undefined) {
      // Constructs that need a backtracking-free matcher stay refused; matching itself is cut off at the request deadline because the engine can backtrack.
      if (UNSUPPORTED.test(this.pattern)) throw new ValidationError('Invalid pathRegex: look-around and backreferences are not supported');
      try {
        new RegExp(this.pattern, 'u');
      } catch (error) {
        throw new ValidationError('Invalid pathRegex: ' + (error as Error).message);
      }
    }
    this.since = input.sinceMs ?? (input.withinMs !== undefined ? Math.max(0, at - input.withinMs) : undefined);
    this.until = input.untilMs;
    if (this.since !== undefined && this.until !== undefined && this.since > this.until) throw new ValidationError('sinceMs must not exceed untilMs');
  }

  /** Decides every candidate path against the pattern in one isolated, interruptible evaluation. */
  prepare(paths: readonly string[], git: Git): void {
    if (this.pattern === undefined) return;
    const unique = [...new Set(paths)].sort(compareCodePoints);
    try {
      const matched: string[] = runInNewContext('const matcher = new RegExp(pattern, "u"); paths.filter(path => matcher.test(path))', { pattern: this.pattern, paths: unique }, { timeout: git.remaining() });
      this.allowed = new Set(matched);
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code === 'ERR_SCRIPT_EXECUTION_TIMEOUT') throw new ValidationError(EXCEEDED);
      throw error;
    }
  }

  path(path: string): boolean {
    return (!this.prefix || path === this.prefix || path.startsWith(this.prefix + '/')) && (this.pattern === undefined || this.allowed!.has(path));
  }

  time(at: number | null): boolean {
    if (at === null) return this.since === undefined && this.until === undefined;
    return (this.since === undefined || at >= this.since) && (this.until === undefined || at <= this.until);
  }
}

function relative(path: string, scope: string): string | null {
  return !scope ? path : path.startsWith(scope + '/') ? path.slice(scope.length + 1) : null;
}

function modified(workspace: string, path: string): number | null {
  try {
    const full = join(workspace, path);
    if (!isWithin(realpath(join(full, '..'), true), workspace)) return null;
    const at = lstatSync(full, { bigint: true }).mtimeNs / 1000000n;
    return at >= 0n ? Number(at) : null;
  } catch {
    return null;
  }
}

const STATES: Record<string, string> = { ' ': 'unchanged', M: 'modified', A: 'added', D: 'deleted', R: 'renamed', C: 'copied', T: 'type-changed', U: 'unmerged', '?': 'untracked', '!': 'ignored' };
const changeState = (code: string): string => STATES[code] ?? 'unknown';

async function files(git: Git, workspace: string, scope: string, filter: Filter): Promise<[FileItem[], number, number]> {
  const records = (await git.read(workspace, ['status', '--porcelain=v1', '-z', '--untracked-files=all', '--', '.'])).split('\0').slice(0, -1);
  const candidates: [string, string, string | null][] = [];
  for (let index = 0; index < records.length; index += 1) {
    const record = records[index]!;
    if (record.length < 4 || record[2] !== ' ') throw new ValidationError('Invalid Git status record');
    const status = record.slice(0, 2);
    const copies = status.includes('R') || status.includes('C');
    const original = copies ? records[++index] : undefined;
    if (copies && original === undefined) throw new ValidationError('Missing rename source');
    const path = relative(record.slice(3), scope);
    if (path !== null) candidates.push([status, path, original !== undefined ? relative(original, scope) : null]);
  }
  filter.prepare(candidates.flatMap(([, path, original]) => (original !== null ? [path, original] : [path])), git);
  const rows: FileItem[] = [];
  let unknown = 0;
  for (const [status, path, original] of candidates) {
    if (!filter.path(path) && !(original !== null && filter.path(original))) continue;
    const at = modified(workspace, path);
    if (at === null) unknown += 1;
    if (filter.time(at)) {
      const index = changeState(status[0]!);
      const worktree = changeState(status[1]!);
      rows.push({ path, status, changes: [...new Set([index, worktree].filter(state => state !== 'unchanged'))], indexState: index, worktreeState: worktree, previousPath: original ?? undefined, modifiedAt: at ?? undefined });
    }
  }
  rows.sort((a, b) => (b.modifiedAt ?? -1) - (a.modifiedAt ?? -1) || compareCodePoints(a.path, b.path));
  return [rows, unknown, candidates.length];
}

/** A Git timestamp in milliseconds; an integer parse with the exact integer range of a JavaScript number. */
function epoch(value: string): number {
  if (!/^\s*[+-]?\d+\s*$/.test(value)) throw new ValidationError(`invalid literal for int() with base 10: '${value}'`);
  const at = BigInt(value.trim()) * 1000n;
  if (at < BigInt(Number.MIN_SAFE_INTEGER) || at > BigInt(Number.MAX_SAFE_INTEGER)) throw new ValidationError('Git timestamp overflow');
  return Number(at);
}

function commits(raw: string, scope: string, filter: Filter, git: Git): Dated[] {
  const fields = raw.split('\0');
  if (fields.at(-1) === '') fields.pop();
  const candidates: [string, number, string, string[]][] = [];
  let index = 0;
  while (index < fields.length) {
    if (index + 3 >= fields.length) throw new ValidationError('Missing commit metadata');
    const hash = fields[index + 1]!;
    const at = epoch(fields[index + 2]!);
    const subject = fields[index + 3]!;
    index += 4;
    const paths: string[] = [];
    let first = true;
    while (index < fields.length && fields[index]) {
      let path = fields[index]!;
      if (first && path.startsWith('\n')) path = path.slice(1);
      first = false;
      const scoped = relative(path, scope);
      if (scoped !== null) paths.push(scoped);
      index += 1;
    }
    candidates.push([hash, at, subject, paths]);
  }
  filter.prepare(candidates.flatMap(([, , , paths]) => paths), git);
  return candidates.map(([hash, at, subject, all]) => {
    const paths = all.filter(path => filter.path(path));
    const matches = paths.length > 0 || (!scope && !filter.prefix && filter.pattern === undefined);
    return { hash, at, subject: truncateCodePoints(subject, TEXT_LIMIT), subjectTruncated: codePointLength(subject) > TEXT_LIMIT, paths: paths.slice(0, FRAGMENT_LIMIT), matchedPathCount: paths.length, pathsTruncated: paths.length > FRAGMENT_LIMIT, matches: matches && filter.time(at) };
  });
}

function reflog(raw: string, filter: Filter): Dated[] {
  const fields = raw.split('\0');
  if (fields.at(-1) === '') fields.pop();
  if (fields.length % 3) throw new ValidationError('Invalid reflog output');
  const rows: Dated[] = [];
  for (let index = 0; index < fields.length; index += 3) {
    const [hash, reference, action] = fields.slice(index, index + 3) as [string, string, string];
    if (!reference.includes('@{') || !reference.endsWith('}')) throw new ValidationError('Missing reflog event time');
    const at = epoch(reference.slice(reference.lastIndexOf('@{') + 2, -1));
    rows.push({ hash, at, action: truncateCodePoints(action, TEXT_LIMIT), actionTruncated: codePointLength(action) > TEXT_LIMIT, matches: filter.time(at) });
  }
  return rows;
}

const BASIS: Record<string, string> = {
  files: 'Current filesystem mtime; not an edit/staging timestamp. Deletions may have unknown time and are excluded by time filters.',
  commits: 'Committer time on current HEAD history; not command execution time.',
  reflog: 'Local HEAD reflog time; reference updates only, not complete Git command history.',
};

export async function read(workspacePath: string, input: ActivityInput): Promise<CommandResults['activity']> {
  const observed = now();
  const filter = new Filter(input, observed);
  const view = input.view ?? 'files';
  if (view === 'reflog' && ('path' in input || 'pathRegex' in input)) throw new ValidationError('Reflog events do not identify changed paths; use view:commits for path filters');
  const workspace = realpath(workspacePath, true);
  const git = new Git();
  const root = realpath((await git.read(workspace, ['rev-parse', '--show-toplevel'])).replace(/\n$/, ''), true);
  if (!isWithin(workspace, root)) throw new ValidationError(`'${workspace}' is not in the subpath of '${root}'`);
  const relation = relativePath(workspace, root);
  const scope = relation === '.' ? '' : relation;
  const cap = input.scanLimit ?? 200;
  let unknown = 0;
  let rows: Item[];
  let truncated = false;
  let scanned: number;
  if (view === 'files') {
    [rows, unknown, scanned] = await files(git, workspace, scope, filter);
  } else {
    const hasHead = (await git.read(workspace, ['rev-parse', '--revs-only', 'HEAD'])).trim() !== '';
    let raw = '';
    if (hasHead && view === 'commits') raw = await git.read(workspace, ['log', '-' + String(cap + 1), '--date-order', '-z', '--name-only', '--format=%x00%H%x00%ct%x00%s', '--no-renames', '--no-ext-diff', '--diff-merges=first-parent', '--full-history']);
    else if (hasHead) raw = await git.read(workspace, ['reflog', 'show', '-' + String(cap + 1), '-z', '--date=unix', '--format=%H%x00%gD%x00%gs', 'HEAD']);
    const found = view === 'commits' ? commits(raw, scope, filter, git) : reflog(raw, filter);
    truncated = found.length > cap;
    const scannedRows = found.slice(0, cap);
    scanned = scannedRows.length;
    rows = scannedRows.filter(row => row.matches).map(({ matches: _matches, ...item }) => item).sort((a, b) => b.at - a.at);
  }
  const digest = createHash('sha256').update(encode(rows)).digest('hex');
  if ('snapshot' in input && input.snapshot !== digest) throw new ValidationError('Activity changed during pagination; restart without after/snapshot');
  const after = input.after ?? 0;
  if (after > rows.length) throw new ValidationError('Activity cursor exceeds result count');
  const count = input.limit ?? 20;
  let end = after;
  let size = 0;
  while (end < rows.length && end - after < count) {
    const length = encode(rows[end]).length;
    if (end > after && size + length > PAGE_BYTES) break;
    size += length;
    end += 1;
  }
  let next: CommandResults['activity']['next'];
  if (end < rows.length) {
    const { withinMs: _withinMs, ...rest } = input;
    next = { command: 'activity', input: { ...rest, ...(filter.since === undefined ? {} : { sinceMs: filter.since }), after: end, snapshot: digest } };
  }
  const unknownTimeQuery = unknown && ['withinMs', 'sinceMs', 'untilMs'].some(key => key in input)
    ? Object.fromEntries(Object.entries(input).filter(([key]) => !['withinMs', 'sinceMs', 'untilMs', 'after', 'snapshot'].includes(key)))
    : undefined;
  return {
    view, workspace, observedAt: observed, items: rows.slice(after, end), totalMatched: rows.length, next,
    coverage: {
      scanned, scanLimit: view === 'files' ? undefined : cap, truncated,
      limitReason: truncated ? 'History scan limit reached; increase scanLimit (max 2000). This is not complete history.' : undefined, unknownFileTimes: unknown, unknownTimeQuery,
    },
    timeBasis: BASIS[view]!, attribution: 'Observed Git/filesystem state does not identify the acting agent.',
    ...(size > PAGE_BYTES ? { budget: { targetBytes: PAGE_BYTES, reason: 'Single oversized row returned intact to preserve evidence and cursor progress' } } : {}),
  };
}
