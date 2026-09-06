/**
 * File read-state tracking and per-file mutation queue.
 *
 * Centralises two shared-state concerns that previously lived in edit-tool.ts
 * but are consumed by write-tool.ts and octocode-tools.ts as well:
 *
 *   1. Read-state map — records content hashes so the edit tool can detect
 *      stale reads before writing (a lost-update guard).
 *   2. Per-file mutation queue — serialises concurrent read-modify-write cycles
 *      on the same file path so parallel tool calls cannot race (within this
 *      process only — see the cross-process note below).
 *
 * Keeping these in one place removes the coupling where write-tool and
 * octocode-tools previously imported from edit-tool.
 *
 * SCOPE — this guard is PROCESS-LOCAL. The read-state map and mutation queue only
 * serialise edits issued within *this* Pi process. They do NOT protect against a
 * second process (for example, a parallel agent worker) editing the same file
 * concurrently. Cross-process safety is a separate layer: declare edited paths
 * via Awareness (`work start`) and take an exclusive lease (`lock acquire`) for
 * non-mergeable or risky shared files — the Awareness pre-edit `tool_call` gate
 * (wired at activation) enforces those leases across processes. See
 * docs/AWARENESS_AGENT_FLOW.md §"Hooks during edits".
 */
import { chmod, mkdir, readFile, realpath, rename, rm, stat, writeFile } from 'node:fs/promises';
import { createHash, randomUUID } from 'node:crypto';
import path from 'node:path';
import { ensurePrivateDirectory, hardenPrivateFile } from '@octocodeai/agent-contracts/permissions';

// ─── Types ────────────────────────────────────────────────────────────────────

interface ReadState {
  mtimeMs: number;
  size: number;
  contentHash: string;
  readAt: number;
}

export interface ReadStateCheck {
  state: 'fresh' | 'missing' | 'stale';
  message: string;
}

// ─── Module-level state ───────────────────────────────────────────────────────

export const MAX_RECORDED_READ_STATES = 1_000;

/**
 * Upper size bound for the mtime+size "fast path" to still fall through to a
 * content-hash comparison. On coarse-mtime filesystems an external same-size
 * rewrite within one mtime tick slips past an mtime+size-only check, so for
 * files at or under this size we always hash-verify (an unchanged file still
 * hash-matches and reports fresh). Above it, hashing is costly and an exact
 * same-size in-tick overwrite is unlikely, so the fast path is preserved.
 */
export const FAST_PATH_HASH_MAX_BYTES = 5 * 1024 * 1024; // 5MB

const readStates = new Map<string, ReadState>();

/**
 * Per-file serialisation queue. Each key is an absolute file path; the value
 * is the settled tail of the promise chain for that file. New operations
 * are appended to the tail and execute after the previous one completes,
 * preserving read-modify-write atomicity without a global lock.
 */
const fileQueues = new Map<string, Promise<void>>();

// ─── Private helpers ──────────────────────────────────────────────────────────

function contentHash(text: string): string {
  return createHash('sha256').update(text).digest('hex');
}

function pruneOldReadStates(): void {
  while (readStates.size > MAX_RECORDED_READ_STATES) {
    const oldestPath = readStates.keys().next().value;
    if (oldestPath === undefined) return;
    readStates.delete(oldestPath);
  }
}

// ─── Path resolution ──────────────────────────────────────────────────────────

/** Resolve a possibly-relative file path against cwd. */
export function resolveFilePath(filePath: string, cwd = process.cwd()): string {
  return path.isAbsolute(filePath) ? filePath : path.resolve(cwd, filePath);
}

// ─── Mutation queue ───────────────────────────────────────────────────────────

/**
 * Run `fn` after all previously-queued mutations on `key` have settled.
 * Errors inside `fn` propagate to the caller but do not stall the queue for
 * future operations.
 */
export function withFileMutationQueue<T>(key: string, fn: () => Promise<T>): Promise<T> {
  const prev = fileQueues.get(key) ?? Promise.resolve();
  const execution = prev.then(() => fn());
  // Tail never rejects — errors propagate via execution, not the queue.
  const tail = execution.then(() => {}, () => {});
  fileQueues.set(key, tail);
  void tail.then(() => {
    if (fileQueues.get(key) === tail) fileQueues.delete(key);
  });
  return execution;
}

// ─── Atomic writes ────────────────────────────────────────────────────────────

/**
 * Write UTF-8 content through a same-directory temp file and atomic rename.
 *
 * Same-directory temp files keep rename atomic on POSIX and avoid cross-device
 * failures. A unique suffix prevents concurrent writers from sharing one temp
 * path; the per-file queue still controls the final write order where needed.
 */
export async function atomicWriteUtf8(filePath: string, content: string, createMode?: number): Promise<void> {
  // Resolve symlinks: temp+rename over a symlinked path would replace the link
  // with a regular file instead of writing through to its target.
  let absolutePath = resolveFilePath(filePath);
  let existingMode: number | undefined;
  try {
    const real = await realpath(absolutePath);
    existingMode = (await stat(real)).mode;
    absolutePath = real;
  } catch {
    // Target doesn't exist yet — plain create with umask defaults.
  }
  await mkdir(path.dirname(absolutePath), { recursive: true });
  const tmpPath = `${absolutePath}.octocode-${process.pid}-${randomUUID()}.tmp`;
  try {
    await writeFile(tmpPath, content, { encoding: 'utf8', ...(createMode === undefined ? {} : { mode: createMode }) });
    // rename resets permissions to the temp file's umask default; preserve the
    // original mode (e.g. exec bits on scripts).
    if (createMode !== undefined) await chmod(tmpPath, createMode);
    else if (existingMode !== undefined) await chmod(tmpPath, existingMode);
    await rename(tmpPath, absolutePath);
  } catch (error) {
    await rm(tmpPath, { force: true }).catch(() => undefined);
    throw error;
  }
}

/** Atomic UTF-8 write for Octocode-home state with owner-only access. */
export async function atomicWritePrivateUtf8(filePath: string, content: string): Promise<void> {
  ensurePrivateDirectory(path.dirname(filePath));
  hardenPrivateFile(filePath);
  await atomicWriteUtf8(filePath, content, 0o600);
  hardenPrivateFile(filePath);
}

// ─── Read-state tracking ──────────────────────────────────────────────────────

/**
 * Record read-state from content already held in memory.
 *
 * Use this instead of recordFileReadState when the caller just wrote the file
 * and already has the content string — it avoids the redundant readFile that
 * recordFileReadState would otherwise issue immediately after an atomic write.
 * Only stat() is needed to capture the post-write mtime and size.
 */
export async function recordFileReadStateFromContent(filePath: string, content: string): Promise<void> {
  const absolutePath = resolveFilePath(filePath);
  const stats = await stat(absolutePath);
  readStates.delete(absolutePath);
  readStates.set(absolutePath, {
    mtimeMs: stats.mtimeMs,
    size: stats.size,
    contentHash: contentHash(content),
    readAt: Date.now(),
  });
  pruneOldReadStates();
}

/** Record a content-hash snapshot of the file for later stale detection. */
export async function recordFileReadState(filePath: string, cwd = process.cwd()): Promise<void> {
  const absolutePath = resolveFilePath(filePath, cwd);
  const [stats, content] = await Promise.all([stat(absolutePath), readFile(absolutePath, 'utf8')]);
  readStates.delete(absolutePath);
  readStates.set(absolutePath, {
    mtimeMs: stats.mtimeMs,
    size: stats.size,
    contentHash: contentHash(content),
    readAt: Date.now(),
  });
  pruneOldReadStates();
}

/** Drop stale-read metadata after a file is deleted. */
export function forgetFileReadState(filePath: string, cwd = process.cwd()): void {
  readStates.delete(resolveFilePath(filePath, cwd));
}

/**
 * Check whether `absolutePath` has changed since the last recorded read.
 *
 * - Fast path: if mtime+size are both unchanged, skip the hash read.
 * - Authoritative path: if they differ, compare content hashes so a
 *   same-content re-write (e.g. editor touch) is NOT falsely reported stale.
 *
 * Throws if `requireRecentRead` is true and no state is recorded.
 */
export async function checkReadState(
  absolutePath: string,
  requireRecentRead: boolean,
  opts: { contentAnchored?: boolean } = {},
): Promise<ReadStateCheck> {
  const state = readStates.get(absolutePath);
  if (!state) {
    const message = 'No prior localGetFileContent read state recorded for this file. Use MCPTool for the read; shell reads do not refresh this guard.';
    if (requireRecentRead) {
      throw new Error(
        `${message} Re-read the file before editing or set requireRecentRead:false intentionally.`,
      );
    }
    return { state: 'missing', message };
  }

  const stats = await stat(absolutePath);
  let stale: boolean;
  if (stats.mtimeMs === state.mtimeMs && stats.size === state.size) {
    // mtime+size match. On coarse-mtime filesystems a same-size external rewrite
    // within one mtime tick can slip past an mtime+size-only check, so fall
    // through to a content-hash comparison for reasonably-sized files (an
    // unchanged file still hash-matches and reports fresh). For very large files
    // hashing is expensive and an exact same-size in-tick overwrite is unlikely,
    // so keep the fast path.
    if (stats.size <= FAST_PATH_HASH_MAX_BYTES) {
      const current = await readFile(absolutePath, 'utf8');
      stale = contentHash(current) !== state.contentHash;
    } else {
      stale = false;
    }
  } else {
    const current = await readFile(absolutePath, 'utf8');
    stale = contentHash(current) !== state.contentHash;
  }
  if (stale) {
    // Content-anchored edits (exact/normalized oldText) are self-verifying: the
    // replacement only applies if oldText still matches the CURRENT bytes, so a
    // stale recorded hash is not a lost-update risk — surface it as advisory
    // rather than blocking. Position-anchored edits (lineRange) and explicit
    // requireRecentRead still hard-fail, since line numbers can silently shift.
    if (opts.contentAnchored && !requireRecentRead) {
      return {
        state: 'stale',
        message: 'File changed since last recorded read; proceeding because the edit is anchored to exact oldText.',
      };
    }
    throw new Error('File changed since last recorded read. Re-read the target range with MCPTool localGetFileContent before editing; shell reads do not refresh this guard.');
  }
  return {
    state: 'fresh',
    message: `Fresh read state recorded ${Math.max(0, Date.now() - state.readAt)}ms ago.`,
  };
}

/**
 * Drop every recorded read state. Called on compaction and session
 * replacement: a hash recorded against a discarded transcript would satisfy
 * the edit tool's stale-read gate while the model's knowledge of the file
 * content is gone — exactly the lost-update the gate exists to prevent.
 */
export function clearAllReadStates(): void {
  readStates.clear();
}

/** Test helper: reset all recorded read states between tests. */
export function clearReadStatesForTests(): void {
  readStates.clear();
}
