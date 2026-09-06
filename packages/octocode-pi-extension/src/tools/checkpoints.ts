/**
 * checkpoints — shadow-git file checkpoints for the Octocode pi extension (F7).
 *
 * REQUIRED WIRING in src/index.ts (this module never edits index.ts itself):
 *
 *   import { initCheckpointStore, type CheckpointEngine } from './tools/checkpoints.js';
 *   import { createCheckpointInputHook, registerRewindCommand } from './tools/rewind-command.js';
 *
 *   let checkpointEngine: Promise<CheckpointEngine | undefined> | undefined;
 *   const getEngine = (ctx?: PiContext) => {
 *     checkpointEngine ??= initCheckpointStore(ctx?.cwd ?? process.cwd()).catch(() => undefined);
 *     return checkpointEngine;
 *   };
 *   pi.on('input', createCheckpointInputHook({ getEngine }));
 *   registerRewindCommand(pi, { getEngine });
 *   // optional hygiene at session start: void getEngine(ctx).then((e) => e?.prune());
 *
 * Design (gemini-cli-style shadow repo):
 * - Store: `<octocodeHome>/checkpoints/<sha256(cwd).slice(0,16)>/`.
 * - Every git command runs via execFile (no shell) with GIT_DIR=<store>/repo.git,
 *   GIT_WORK_TREE=<cwd>, and a PRIVATE GIT_INDEX_FILE=<store>/index. The user's own
 *   .git — index, HEAD, refs, hooks — is NEVER touched or even read for writing.
 *   That isolation is the #1 invariant of this module.
 * - GIT_CONFIG_GLOBAL/GIT_CONFIG_SYSTEM point at os.devNull so host git config
 *   (gpg signing, hook templates, odd excludes) cannot leak into snapshots.
 * - `.gitignore` files inside the work tree are respected automatically by git;
 *   init additionally links `core.excludesFile` to the user repo's
 *   `.git/info/exclude` (read-only) when one exists.
 * - Latency guard: if a snapshot ever takes longer than the guard (default 10s),
 *   the engine self-disables for the rest of the process (pathological trees).
 * - prune(keep) shortens history by marking the oldest kept commit as shallow —
 *   `git log`/`rev-list` then see exactly `keep` commits.
 */

import { execFile } from 'node:child_process';
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import { mkdir, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { promisify } from 'node:util';
import { extensionHome } from '../extension-paths.js';
import { ensurePrivateDirectory } from '@octocodeai/agent-contracts/permissions';

const execFileAsync = promisify(execFile);

const DEFAULT_LATENCY_GUARD_MS = 10_000;
const DEFAULT_PRUNE_KEEP = 30;
const ENTRY_TRAILER = 'octocode-entry:';

// ─── Public types ────────────────────────────────────────────────────────────

export interface SnapshotResult {
  /** Full commit sha in the shadow repo. */
  id: string;
  filesChanged: number;
}

export interface CheckpointInfo {
  id: string;
  label: string;
  /** Commit time, epoch milliseconds. */
  ts: number;
  filesChanged: number;
  /** Pi session entry id captured when the snapshot was taken (conversation rewind target). */
  entryId?: string;
}

export interface DiffStatEntry {
  /** git name-status letter (M/A/D/R/…). */
  status: string;
  path: string;
}

export interface CheckpointEngine {
  readonly cwd: string;
  readonly storeDir: string;
  /** True once the latency guard tripped — every later snapshot returns undefined. */
  isDisabled(): boolean;
  snapshot(label: string, opts?: { entryId?: string }): Promise<SnapshotResult | undefined>;
  listCheckpoints(limit?: number): Promise<CheckpointInfo[]>;
  restoreFiles(id: string, paths?: string[]): Promise<void>;
  /** name-status of checkpoint `id` vs the current work tree. */
  diffStat(id: string): Promise<DiffStatEntry[]>;
  prune(keep?: number): Promise<void>;
}

export interface CheckpointStoreOptions {
  /** Snapshot latency guard in ms (default 10s). Injectable for tests. */
  latencyGuardMs?: number;
}

// ─── Store location ──────────────────────────────────────────────────────────

/** `<home>/checkpoints/<sha256(resolved cwd).slice(0,16)>` — never inside the user repo. */
export function checkpointStoreDir(cwd: string, home?: string): string {
  const key = createHash('sha256').update(path.resolve(cwd)).digest('hex').slice(0, 16);
  return path.join(extensionHome(home), 'checkpoints', key);
}

// ─── Internals ───────────────────────────────────────────────────────────────

/**
 * Environment for every shadow-repo git call. Overrides/deletes anything that
 * could redirect git at the user's repository or host configuration.
 */
function shadowEnv(storeDir: string, cwd: string): NodeJS.ProcessEnv {
  const env: NodeJS.ProcessEnv = { ...process.env };
  // Never inherit repo-redirection vars from the host process.
  delete env.GIT_OBJECT_DIRECTORY;
  delete env.GIT_ALTERNATE_OBJECT_DIRECTORIES;
  delete env.GIT_COMMON_DIR;
  delete env.GIT_NAMESPACE;
  delete env.GIT_CEILING_DIRECTORIES;
  env.GIT_DIR = path.join(storeDir, 'repo.git');
  env.GIT_WORK_TREE = cwd;
  env.GIT_INDEX_FILE = path.join(storeDir, 'index'); // private index — user index NEVER touched
  env.GIT_CONFIG_GLOBAL = os.devNull;
  env.GIT_CONFIG_SYSTEM = os.devNull;
  env.GIT_TERMINAL_PROMPT = '0';
  env.GIT_AUTHOR_NAME = 'octocode';
  env.GIT_AUTHOR_EMAIL = 'octocode@localhost';
  env.GIT_COMMITTER_NAME = 'octocode';
  env.GIT_COMMITTER_EMAIL = 'octocode@localhost';
  return env;
}

function assertCommitish(id: string): void {
  if (!/^[0-9a-fA-F]{4,40}$/.test(id)) {
    throw new Error(`invalid checkpoint id: ${JSON.stringify(id)}`);
  }
}

function countLines(out: string): number {
  return out.split('\n').filter((l) => l.trim().length > 0).length;
}

// ─── Engine ──────────────────────────────────────────────────────────────────

/**
 * Initialize (idempotently) the shadow checkpoint store for `cwd` and return
 * the engine. Throws when git is unavailable or the store cannot be created —
 * callers should catch and treat checkpoints as unavailable.
 */
export async function initCheckpointStore(
  cwd: string,
  home?: string,
  opts?: CheckpointStoreOptions,
): Promise<CheckpointEngine> {
  const resolvedCwd = path.resolve(cwd);
  const storeDir = checkpointStoreDir(resolvedCwd, home);
  const gitDir = path.join(storeDir, 'repo.git');
  const env = shadowEnv(storeDir, resolvedCwd);
  const guardMs = opts?.latencyGuardMs ?? DEFAULT_LATENCY_GUARD_MS;
  const execTimeout = guardMs > 0 ? guardMs : undefined;

  let disabled = false;
  // Mutating operations share the private index file — serialize them.
  let chain: Promise<unknown> = Promise.resolve();
  const withLock = <T>(fn: () => Promise<T>): Promise<T> => {
    const run = chain.then(fn, fn);
    chain = run.then(
      () => undefined,
      () => undefined,
    );
    return run;
  };

  const git = async (args: string[], timeout?: number): Promise<string> => {
    const { stdout } = await execFileAsync('git', args, {
      cwd: resolvedCwd,
      env,
      timeout: timeout ?? execTimeout,
      maxBuffer: 16 * 1024 * 1024,
      windowsHide: true,
    });
    return stdout;
  };

    await mkdir(gitDir, { recursive: true, mode: 0o700 });
    ensurePrivateDirectory(gitDir);
  // `git init` is idempotent on an existing repo; -c only affects fresh init.
  await git(['-c', 'init.defaultBranch=main', 'init', '--quiet']);
  // Respect the user repo's local excludes (read-only) in addition to the
  // .gitignore files git already honors inside the work tree.
  const userExclude = path.join(resolvedCwd, '.git', 'info', 'exclude');
  if (fs.existsSync(userExclude)) {
    await git(['config', 'core.excludesFile', userExclude]);
  }

  const listCheckpoints = async (limit = DEFAULT_PRUNE_KEEP): Promise<CheckpointInfo[]> => {
    let raw: string;
    try {
      raw = await git(['log', '-n', String(limit), '--format=%H%x1f%ct%x1f%s%x1f%b%x1e']);
    } catch {
      return []; // no commits yet (or unreadable history)
    }
    const out: CheckpointInfo[] = [];
    for (const chunk of raw.split('\x1e')) {
      const record = chunk.replace(/^\s+/, '');
      if (!record) continue;
      const [id, ct, subject, body = ''] = record.split('\x1f');
      if (!id || !ct) continue;
      const entryMatch = /(?:^|\n)octocode-entry:[ \t]*(\S+)/.exec(body);
      const info: CheckpointInfo = {
        id,
        label: subject ?? '',
        ts: Number.parseInt(ct, 10) * 1000,
        filesChanged: 0,
      };
      if (entryMatch) info.entryId = entryMatch[1];
      out.push(info);
    }
    // Populate filesChanged concurrently: one diff-tree per checkpoint (up to
    // DEFAULT_PRUNE_KEEP=30) would otherwise be 30 sequential child processes,
    // a latency cliff on large/slow filesystems.
    await Promise.all(
      out.map(async (info) => {
        try {
          const names = await git(['diff-tree', '--root', '--no-commit-id', '--name-only', '-r', info.id]);
          info.filesChanged = countLines(names);
        } catch {
          // keep 0 — metadata still useful
        }
      }),
    );
    return out;
  };

  const engine: CheckpointEngine = {
    cwd: resolvedCwd,
    storeDir,
    isDisabled: () => disabled,

    snapshot: (label, snapOpts) =>
      withLock(async () => {
        if (disabled) return undefined;
        const started = Date.now();
        try {
          await git(['add', '-A']);
          const commitArgs = [
            'commit',
            '--quiet',
            '--allow-empty',
            '--allow-empty-message',
            '--no-verify',
            '-m',
            label,
          ];
          if (snapOpts?.entryId) commitArgs.push('-m', `${ENTRY_TRAILER} ${snapOpts.entryId}`);
          await git(commitArgs);
          const id = (await git(['rev-parse', 'HEAD'])).trim();
          const names = await git(['diff-tree', '--root', '--no-commit-id', '--name-only', '-r', 'HEAD']);
          if (guardMs >= 0 && Date.now() - started > guardMs) {
            disabled = true; // latency guard tripped — checkpointing off for this session
            return undefined;
          }
          return { id, filesChanged: countLines(names) };
        } catch {
          // A timed-out git call means the guard window was blown mid-flight.
          if (guardMs >= 0 && Date.now() - started > guardMs) disabled = true;
          return undefined;
        }
      }),

    listCheckpoints,

    restoreFiles: (id, paths) =>
      withLock(async () => {
        assertCommitish(id);
        const targets = paths && paths.length > 0 ? paths : ['.'];
        // Writes only the work tree + the PRIVATE shadow index; user .git untouched.
        await git(['checkout', id, '--', ...targets]);
        // `git checkout <id> -- .` reverts tracked paths but NEVER deletes files
        // created after the checkpoint, so the work tree would not actually match
        // the snapshot the user rewound to. Remove the paths that are tracked-added
        // relative to <id> (untracked scratch files are not in the shadow index, so
        // this leaves them alone) to complete a true restore.
        try {
          const added = await git(['diff', '--name-only', '-z', '--diff-filter=A', id, '--', ...targets]);
          // -z output is raw/unquoted precisely so paths with spaces survive; do
          // NOT .trim() — a filename with a legal leading/trailing space would be
          // mangled and the wrong path (or none) removed. filter(Boolean) only
          // drops the empty element after the final NUL.
          const rels = added.split('\0').filter(Boolean);
          for (const rel of rels) {
            try {
              fs.rmSync(path.join(resolvedCwd, rel), { force: true });
            } catch {
              // best-effort: a path we cannot remove is left as-is
            }
          }
          // Keep the shadow index consistent with the reverted work tree.
          if (rels.length > 0) {
            await git(['rm', '--cached', '--force', '--quiet', '--ignore-unmatch', '--', ...rels]).catch(() => undefined);
          }
        } catch {
          // If listing added paths fails, the checkout above still applied — do not
          // fail the whole restore over the post-checkpoint cleanup.
        }
      }),

    diffStat: async (id) => {
      assertCommitish(id);
      const raw = await git(['diff', '--name-status', id, '--', '.']);
      const entries: DiffStatEntry[] = [];
      for (const line of raw.split('\n')) {
        if (!line.trim()) continue;
        const parts = line.split('\t');
        if (parts.length < 2) continue;
        const status = (parts[0] ?? '').charAt(0) || '?';
        const filePath = parts[parts.length - 1] ?? '';
        entries.push({ status, path: filePath });
      }
      return entries;
    },

    prune: (keep = DEFAULT_PRUNE_KEEP) =>
      withLock(async () => {
        if (keep < 1) return;
        let count = 0;
        try {
          count = Number.parseInt((await git(['rev-list', '--count', 'HEAD'])).trim(), 10) || 0;
        } catch {
          return; // no commits yet
        }
        if (count <= keep) return;
        // Mark the oldest kept commit as shallow: history now ends there.
        const boundary = (await git(['rev-parse', `HEAD~${keep - 1}`])).trim();
        await writeFile(path.join(gitDir, 'shallow'), `${boundary}\n`, 'utf8');
        // Best-effort object cleanup; correctness does not depend on it.
        try {
          await git(['reflog', 'expire', '--expire=now', '--all']);
          await git(['gc', '--prune=now', '--quiet']);
        } catch {
          // ignore — shallow file alone already enforces the keep window
        }
      }),
  };

  return engine;
}
