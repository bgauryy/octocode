import { lstat, realpath } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import type { MutationReceipt, NativeCancellation } from '@octocodeai/octocode-extension-rust';
import { loadNativeFiles, withNativeFiles } from './native-files.js';
import { dirname, isAbsolute, relative, resolve, sep } from 'node:path';
import type { HistoryFileMode } from './history-git.js';

export type CaptureOmissionReason = 'excluded' | 'symlink' | 'unsupported_type' | 'file_too_large' | 'batch_too_large' | 'file_limit' | 'unstable';

export type WorkspaceFileSnapshot =
  | { path: string; status: 'captured'; mode: HistoryFileMode; bytes: Uint8Array; digest: string; size: number }
  | { path: string; status: 'missing'; digest: string; size: 0 }
  | { path: string; status: 'omitted' | 'unstable'; reason: CaptureOmissionReason; size?: number };

export interface CapturePolicy {
  maxFiles?: number;
  maxFileBytes?: number;
  maxBatchBytes?: number;
  include?: string[];
}

export interface CaptureWorkspaceFilesOptions {
  workspace: string;
  paths: string[];
  policy?: CapturePolicy;
  signal?: AbortSignal;
}

export interface CaptureBatch {
  workspace: string;
  entries: WorkspaceFileSnapshot[];
  capturedBytes: number;
}

export interface RestoreWorkspaceFileOptions {
  workspace: string;
  path: string;
  expectedCurrent: ExpectedWorkspaceFileSnapshot;
  target: RestoreFileTarget;
  signal?: AbortSignal;
}

export interface ExpectedWorkspaceFileSnapshot {
  path: string;
  status: 'captured' | 'missing';
  digest: string;
  size: number;
  mode?: HistoryFileMode;
}

export type RestoreFileTarget =
  | { path: string; status: 'captured'; mode: HistoryFileMode; bytes: Uint8Array }
  | { path: string; status: 'missing' };

export interface RestoreWorkspaceFileResult {
  path: string;
  status: 'restored' | 'deleted';
  previous: WorkspaceFileSnapshot;
  current: WorkspaceFileSnapshot;
  receipt: Omit<MutationReceipt, 'committed'> & { committed: boolean };
  warnings: string[];
}

const DEFAULT_MAX_FILES = 200;
const DEFAULT_MAX_FILE_BYTES = 2 * 1024 * 1024;
const DEFAULT_MAX_BATCH_BYTES = 16 * 1024 * 1024;
const MISSING_DIGEST = createHash('sha256').update('octocode:missing\0').digest('hex');
const EXCLUDED_SEGMENTS = new Set(['.git', '.octocode', 'node_modules', 'dist', 'out', 'target']);

// Tokens stay private to the exact captured object; persisted previews retain
// content/mode semantics and acquire a fresh native token when applying.
const versions = new WeakMap<WorkspaceFileSnapshot, string>();
type NativeFiles = Awaited<ReturnType<typeof loadNativeFiles>>;

function normalizeRelativePath(workspace: string, input: string, workspaceAlias = workspace): { path: string; absolute: string } {
  if (!input || input.includes('\0') || /[\uD800-\uDBFF](?![\uDC00-\uDFFF])|(?<![\uD800-\uDBFF])[\uDC00-\uDFFF]/u.test(input)) throw new Error(`invalid workspace path: ${JSON.stringify(input)}`);
  let absolute = resolve(workspace, input);
  let rel = relative(workspace, absolute);
  if (isAbsolute(input) && (rel === '..' || rel.startsWith(`..${sep}`) || isAbsolute(rel))) {
    const aliasRelative = relative(workspaceAlias, resolve(input));
    if (aliasRelative !== '..' && !aliasRelative.startsWith(`..${sep}`) && !isAbsolute(aliasRelative)) {
      rel = aliasRelative;
      absolute = resolve(workspace, aliasRelative);
    }
  }
  if (!rel || rel === '..' || rel.startsWith(`..${sep}`) || isAbsolute(rel)) {
    if (!rel) throw new Error('workspace root is not a capturable file');
    throw new Error(`path is outside workspace: ${JSON.stringify(input)}`);
  }
  return { path: rel.split(sep).join('/'), absolute };
}

function isExcluded(path: string, includes: Set<string>): boolean {
  if (includes.has(path)) return false;
  const segments = path.split('/');
  const leaf = segments.at(-1) ?? '';
  return segments.some(segment => EXCLUDED_SEGMENTS.has(segment))
    || /^\.env(?:\.|$)/i.test(leaf)
    || /\.(?:pem|key)$/i.test(leaf);
}

function gitMode(mode: number): HistoryFileMode {
  return (mode & 0o111) === 0 ? '100644' : '100755';
}

async function captureOne(
  workspace: string,
  input: string,
  maxFileBytes: number,
  remainingBatchBytes: number,
  includes: Set<string>,
  native: NativeFiles,
  cancellation: NativeCancellation,
): Promise<WorkspaceFileSnapshot> {
  const normalized = normalizeRelativePath(workspace, input);
  if (isExcluded(normalized.path, includes)) return { path: normalized.path, status: 'omitted', reason: 'excluded' };
  // Host policy supplies omission reasons; native descriptors enforce the actual
  // no-follow boundary even when entries change after this observation.
  if (await hasSymlinkAncestor(workspace, normalized.absolute)) return { path: normalized.path, status: 'omitted', reason: 'symlink' };
  const before = await lstat(normalized.absolute, { bigint: true }).catch((error: NodeJS.ErrnoException) => {
    if (error.code === 'ENOENT') return undefined;
    throw error;
  });
  if (before?.isSymbolicLink()) return { path: normalized.path, status: 'omitted', reason: 'symlink', size: Number(before.size) };
  if (before && !before.isFile()) return { path: normalized.path, status: 'omitted', reason: 'unsupported_type', size: Number(before.size) };
  if (before && before.size > BigInt(maxFileBytes)) return { path: normalized.path, status: 'omitted', reason: 'file_too_large', size: Number(before.size) };
  if (before && before.size > BigInt(remainingBatchBytes)) return { path: normalized.path, status: 'omitted', reason: 'batch_too_large', size: Number(before.size) };
  try {
    const snapshot = await native.snapshotFile(normalized.absolute, Math.min(maxFileBytes, remainingBatchBytes), true, false, cancellation);
    const result: WorkspaceFileSnapshot = snapshot.exists
      ? { path: normalized.path, status: 'captured', mode: gitMode(snapshot.mode ?? 0o666), bytes: snapshot.content!, digest: snapshot.digest!, size: snapshot.size }
      : { path: normalized.path, status: 'missing', digest: MISSING_DIGEST, size: 0 };
    versions.set(result, snapshot.version);
    return result;
  } catch (error) {
    const code = native.nativeErrorCode(error);
    if (code === native.NativeErrorCodes.TOO_LARGE) return { path: normalized.path, status: 'omitted', reason: remainingBatchBytes < maxFileBytes ? 'batch_too_large' : 'file_too_large' };
    if (code === native.NativeErrorCodes.PRECONDITION_FAILED) return { path: normalized.path, status: 'unstable', reason: 'unstable' };
    if (code === native.NativeErrorCodes.NOT_REGULAR_FILE || code === native.NativeErrorCodes.UNSAFE_PATH) return { path: normalized.path, status: 'unstable', reason: 'unstable' };
    throw error;
  }
}

export async function captureWorkspaceFiles(options: CaptureWorkspaceFilesOptions): Promise<CaptureBatch> {
  const workspaceAlias = resolve(options.workspace);
  const workspace = await realpath(workspaceAlias);
  const maxFiles = options.policy?.maxFiles ?? DEFAULT_MAX_FILES;
  const maxFileBytes = options.policy?.maxFileBytes ?? DEFAULT_MAX_FILE_BYTES;
  const maxBatchBytes = options.policy?.maxBatchBytes ?? DEFAULT_MAX_BATCH_BYTES;
  if (!Number.isSafeInteger(maxFiles) || maxFiles < 1 || !Number.isSafeInteger(maxFileBytes) || maxFileBytes < 0 || maxFileBytes > 0xffff_ffff || !Number.isSafeInteger(maxBatchBytes) || maxBatchBytes < 0) {
    throw new Error('invalid capture budget');
  }
  const includes = new Set((options.policy?.include ?? []).map(value => normalizeRelativePath(workspace, value, workspaceAlias).path));
  const entries: WorkspaceFileSnapshot[] = [];
  const seen = new Set<string>();
  let capturedBytes = 0;
  return withNativeFiles(options.signal, async (native, cancellation) => {
    for (const input of options.paths) {
      options.signal?.throwIfAborted();
      const normalized = normalizeRelativePath(workspace, input, workspaceAlias);
      if (seen.has(normalized.path)) continue;
      seen.add(normalized.path);
      if (entries.length >= maxFiles) {
        entries.push({ path: normalized.path, status: 'omitted', reason: 'file_limit' });
        continue;
      }
      const snapshot = await captureOne(workspace, normalized.path, maxFileBytes, maxBatchBytes - capturedBytes, includes, native, cancellation);
      entries.push(snapshot);
      if (snapshot.status === 'captured') capturedBytes += snapshot.size;
    }
    return { workspace, entries, capturedBytes };
  });
}

function equalForCas(actual: WorkspaceFileSnapshot, expected: ExpectedWorkspaceFileSnapshot): boolean {
  if (actual.status !== expected.status || actual.path !== expected.path) return false;
  if (actual.status === 'missing' && expected.status === 'missing') return true;
  if (actual.status === 'captured' && expected.status === 'captured') {
    return actual.digest === expected.digest && actual.mode === expected.mode && actual.size === expected.size;
  }
  return false;
}

async function assertNoSymlinkAncestors(workspace: string, absolute: string): Promise<void> {
  const rel = relative(workspace, dirname(absolute));
  if (!rel) return;
  let cursor = workspace;
  for (const segment of rel.split(sep)) {
    cursor = resolve(cursor, segment);
    try {
      if ((await lstat(cursor)).isSymbolicLink()) throw new Error(`restore path has symlink ancestor: ${cursor}`);
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code === 'ENOENT') return;
      throw error;
    }
  }
}

async function hasSymlinkAncestor(workspace: string, absolute: string): Promise<boolean> {
  try {
    await assertNoSymlinkAncestors(workspace, absolute);
    return false;
  } catch (error) {
    if (error instanceof Error && error.message.startsWith('restore path has symlink ancestor:')) return true;
    throw error;
  }
}

export async function restoreWorkspaceFile(options: RestoreWorkspaceFileOptions): Promise<RestoreWorkspaceFileResult> {
  const workspace = await realpath(resolve(options.workspace));
  const targetPath = normalizeRelativePath(workspace, options.path);
  if (options.expectedCurrent.path !== targetPath.path || options.target.path !== targetPath.path) throw new Error('restore path does not match snapshots');
  if (options.expectedCurrent.status !== 'captured' && options.expectedCurrent.status !== 'missing') throw new Error('omitted or unstable snapshots are not recoverable');
  const actual = (await captureWorkspaceFiles({ workspace, paths: [targetPath.path], policy: { include: [targetPath.path] }, signal: options.signal })).entries[0]!;
  if (!equalForCas(actual, options.expectedCurrent)) throw new Error(`stale restore preview for ${targetPath.path}`);
  const version = versions.get(actual);
  if (!version) throw new Error(`stale restore preview for ${targetPath.path}: no native snapshot`);
  if (options.target.status === 'missing' && actual.status === 'missing') {
    return { path: targetPath.path, status: 'deleted', previous: actual, current: actual,
      receipt: { committed: false, durable: false, bytes: 0, warnings: [] }, warnings: [] };
  }
  const receipt = await withNativeFiles(options.signal, async (native, cancellation) => {
    try {
      if (options.target.status === 'missing') return await native.deleteFile(targetPath.absolute, version, DEFAULT_MAX_FILE_BYTES, cancellation);
      const bytes = Buffer.from(options.target.bytes.buffer, options.target.bytes.byteOffset, options.target.bytes.byteLength);
      const mode = process.platform === 'win32' ? undefined : options.target.mode === '100755' ? 0o755 : 0o644;
      return await native.replaceFile(targetPath.absolute, bytes, version, DEFAULT_MAX_FILE_BYTES, mode, cancellation, 0o700);
    } catch (error) {
      if (native.nativeErrorCode(error) === native.NativeErrorCodes.PRECONDITION_FAILED) throw new Error(`stale restore preview for ${targetPath.path}`, { cause: error });
      throw error;
    }
  });
  const warnings = [...receipt.warnings];
  if (process.platform === 'win32' && options.target.status === 'captured' && options.target.mode === '100755') warnings.push('File restored; Windows does not apply Unix executable mode bits.');
  let current: WorkspaceFileSnapshot;
  try {
    // No postcommit abort: cancellation cannot erase the completed mutation.
    current = (await captureWorkspaceFiles({ workspace, paths: [targetPath.path], policy: { include: [targetPath.path] } })).entries[0]!;
    if (current.status === 'omitted' || current.status === 'unstable') warnings.push(`File committed; postcommit observation is ${current.status}.`);
  } catch (error) {
    current = { path: targetPath.path, status: 'unstable', reason: 'unstable' };
    warnings.push(`File committed; postcommit observation failed: ${error instanceof Error ? error.message : String(error)}`);
  }
  return { path: targetPath.path, status: options.target.status === 'missing' ? 'deleted' : 'restored', previous: actual, current, receipt, warnings };
}
