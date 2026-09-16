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
 * via Awareness (`work.create`) and take exceptional exclusive protection (`work.protect`) for
 * non-mergeable or risky shared files — the Awareness pre-edit `tool_call` gate
 * (wired at activation) enforces those leases across processes. See
 * docs/AWARENESS_AGENT_FLOW.md §"Hooks during edits".
 */
import { stat } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import path from 'node:path';
import { ensurePrivateDirectory, hardenPrivateFile } from '@octocodeai/agent-contracts/permissions';
import { canonicalPathKey, resolveCanonicalPath } from './path-guard.js';
import { assertFileContentSize, replaceNativeFile, snapshotNativeFile } from './native-files.js';
import type { MutationReceipt } from '@octocodeai/octocode-extension-rust';

// ─── Types ────────────────────────────────────────────────────────────────────

interface ReadState {
  contentHash: string;
  readAt: number;
}

export interface ReadStateCheck {
  state: 'fresh' | 'missing' | 'stale';
  message: string;
}

// ─── Module-level state ───────────────────────────────────────────────────────

export const MAX_RECORDED_READ_STATES = 1_000;

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
  return path.resolve(cwd, filePath);
}

// ─── Mutation queue ───────────────────────────────────────────────────────────

/**
 * Run `fn` after all previously-queued mutations on `key` have settled.
 * Errors inside `fn` propagate to the caller but do not stall the queue for
 * future operations.
 */
export function withFileMutationQueue<T>(key: string, fn: () => Promise<T>): Promise<T> {
  key = canonicalPathKey(resolveCanonicalPath(key));
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
 * Native durable replacement for internal state; queue snapshot and commit
 * together. Public file tools carry an earlier preflight snapshot themselves.
 */
export async function atomicWriteUtf8(filePath: string, content: string, createMode?: number): Promise<MutationReceipt> {
  assertFileContentSize(content);
  const absolutePath = resolveCanonicalPath(filePath);
  return withFileMutationQueue(absolutePath, async () => {
    const snapshot = await snapshotNativeFile(absolutePath);
    return replaceNativeFile(absolutePath, content, snapshot.version, undefined, createMode);
  });
}

/** Atomic UTF-8 write for Octocode-home state with owner-only access. */
export async function atomicWritePrivateUtf8(filePath: string, content: string): Promise<MutationReceipt> {
  ensurePrivateDirectory(path.dirname(filePath));
  hardenPrivateFile(filePath);
  return atomicWriteUtf8(filePath, content, 0o600);
}

// ─── Read-state tracking ──────────────────────────────────────────────────────

/**
 * Record read-state from content already held in memory.
 *
 * Use this instead of recordFileReadState when the caller just wrote the file
 * and already has the content string — it avoids the redundant readFile that
 * recordFileReadState would otherwise issue immediately after an atomic write.
 * A stat confirms the path still exists; future checks compare content hashes.
 */
export async function recordFileReadStateFromContent(filePath: string, content: string): Promise<void> {
  const absolutePath = resolveCanonicalPath(resolveFilePath(filePath));
  await stat(absolutePath);
  readStates.delete(absolutePath);
  readStates.set(absolutePath, {
    contentHash: contentHash(content),
    readAt: Date.now(),
  });
  pruneOldReadStates();
}

/** Record a content-hash snapshot of the file for later stale detection. */
export async function recordFileReadState(filePath: string, cwd = process.cwd()): Promise<void> {
  const absolutePath = resolveCanonicalPath(resolveFilePath(filePath, cwd));
  const snapshot = await snapshotNativeFile(absolutePath);
  if (!snapshot.exists || !snapshot.digest) throw new Error(`File not found: ${filePath}`);
  readStates.delete(absolutePath);
  readStates.set(absolutePath, {
    contentHash: snapshot.digest,
    readAt: Date.now(),
  });
  pruneOldReadStates();
}

/** Drop stale-read metadata after a file is deleted. */
export function forgetFileReadState(filePath: string, cwd = process.cwd()): void {
  readStates.delete(resolveCanonicalPath(resolveFilePath(filePath, cwd)));
}

/**
 * Check whether `absolutePath` has changed since the last recorded read.
 *
 * Compare authoritative hashes, using the prepared edit's snapshot when
 * available. Same-content rewrites are not falsely reported stale.
 *
 * Throws if `requireRecentRead` is true and no state is recorded.
 */
export async function checkReadState(
  absolutePath: string,
  requireRecentRead: boolean,
  opts: { contentAnchored?: boolean; currentDigest?: string; implicitReason?: string } = {},
): Promise<ReadStateCheck> {
  absolutePath = resolveCanonicalPath(absolutePath);
  const state = readStates.get(absolutePath);
  if (!state) {
    const message = 'No prior localFetch read state recorded for this file. Shell reads (bash/cat/grep) do not refresh this guard — use MCPTool localFetch instead: MCPTool(action:"call",server:"octocode",tool:"localFetch",arguments:{queries:[{path:"<absolute_path>"}]}).';
    if (requireRecentRead) {
      const why = opts.implicitReason ? ` Implicit cause: ${opts.implicitReason}.` : '';
      throw new Error(
        `${message}${why} Re-read the file via MCPTool before editing${opts.contentAnchored === false ? ', or provide oldText matching the requested range.' : ', or set requireRecentRead:false only when intentional.'}`,
      );
    }
    return { state: 'missing', message };
  }

  const currentDigest = opts.currentDigest ?? (await snapshotNativeFile(absolutePath)).digest;
  const stale = currentDigest !== state.contentHash;
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
    throw new Error('File changed since last recorded read. Re-read the target range via MCPTool localFetch before editing (shell reads do not refresh this guard): MCPTool(action:"call",server:"octocode",tool:"localFetch",arguments:{queries:[{path:"<absolute_path>"}]}).');
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
