/// <reference types="node" />

export const NativeErrorCodes: {
  readonly CANCELLED: 'CANCELLED';
  readonly HISTORY_OBJECT_HASH_MISMATCH: 'HISTORY_OBJECT_HASH_MISMATCH';
  readonly HISTORY_OBJECT_INVALID: 'HISTORY_OBJECT_INVALID';
  readonly HISTORY_OBJECT_LIMIT: 'HISTORY_OBJECT_LIMIT';
  readonly HISTORY_OBJECT_TIME_LIMIT: 'HISTORY_OBJECT_TIME_LIMIT';
  readonly HISTORY_OBJECT_UNAVAILABLE: 'HISTORY_OBJECT_UNAVAILABLE';
  readonly INVALID_MODE: 'INVALID_MODE';
  readonly INVALID_PATH: 'INVALID_PATH';
  readonly INVALID_TEXT: 'INVALID_TEXT';
  readonly IO_FAILURE: 'IO_FAILURE';
  readonly NOT_REGULAR_FILE: 'NOT_REGULAR_FILE';
  readonly PRECONDITION_FAILED: 'PRECONDITION_FAILED';
  readonly TOO_LARGE: 'TOO_LARGE';
  readonly UNSAFE_PATH: 'UNSAFE_PATH';
  readonly UNSUPPORTED_PLATFORM: 'UNSUPPORTED_PLATFORM';
};
export type NativeErrorCode = typeof NativeErrorCodes[keyof typeof NativeErrorCodes];
export class NativeOperationError extends Error {
  constructor(code: NativeErrorCode, message: string, cause?: unknown);
  readonly code: NativeErrorCode;
}
/** Classify native and validation errors without consumers parsing message text. */
export function nativeErrorCode(error: unknown): NativeErrorCode | undefined;

export interface FileSnapshot {
  exists: boolean;
  kind: 'file' | 'symlink' | 'missing';
  version: string;
  digest?: string;
  content?: Buffer;
  /** Unix permission bits; absent on Windows, where ACLs govern access. */
  mode?: number;
  size: number;
}
export interface MutationReceipt {
  committed: true;
  /** Directory-entry sync succeeded. Windows always returns false with a warning. */
  durable: boolean;
  warnings: string[];
  bytes: number;
}
export class NativeCancellation {
  constructor();
  cancel(): void;
}
export interface GitObject {
  objectType: 'blob' | 'tree' | 'commit' | 'tag';
  size: number;
  content?: Buffer;
}
export interface FileDurability { durable: boolean; warnings: string[] }
/** Flush a pinned regular file and ancestor entries. Windows flushes data and returns durable:false. */
export function flushFile(path: string, cancellation?: NativeCancellation): Promise<FileDurability>;
/** Create missing directory ancestors privately through pinned no-follow traversal; existing modes are unchanged. */
export function ensurePrivateDirectory(path: string, cancellation?: NativeCancellation): Promise<void>;
/** Streaming authenticated loose-object read; fixed 64 KiB scratch buffers, explicit compressed/decoded caps, deadline includes queue time. */
export function readGitObject(path: string, oid: string, maxDecodedBytes: number, maxCompressedBytes: number, timeoutMs: number, includeContent?: boolean, cancellation?: NativeCancellation): Promise<GitObject>;
export function snapshotFile(path: string, maxBytes: number, includeContent: boolean, allowLeafSymlink?: boolean, cancellation?: NativeCancellation): Promise<FileSnapshot>;
export interface FileFingerprint {
  fingerprint?: string;
  reason?: string;
  paths: string[];
  files: number;
  bytes: number;
}
/** Exact Awareness v1 fingerprint; failure never returns a partial fingerprint. Deadline includes worker queue time. */
export function fingerprintFiles(root: string, paths: string[], maxFileBytes: number, maxBatchBytes: number, maxFiles: number, timeoutMs: number, cancellation?: NativeCancellation): Promise<FileFingerprint>;
/** createMode overrides Unix permissions; Windows accepts only 0600 (private owner/SYSTEM DACL). */
export function replaceFile(path: string, content: Buffer, expectedVersion: string, maxBytes: number, createMode?: number, cancellation?: NativeCancellation, parentMode?: number): Promise<MutationReceipt>;
export function deleteFile(path: string, expectedVersion: string, maxBytes: number, cancellation?: NativeCancellation): Promise<MutationReceipt>;
export interface DiffOperation {
  opType: 'same' | 'add' | 'remove';
  line: string;
}
export function computeLineDiff(oldText: string, newText: string): DiffOperation[];
export function computeLineDiffAsync(oldText: string, newText: string): Promise<DiffOperation[]>;
export interface DiffArtifacts {
  diff: string;
  patch: string;
}
export function generateDiffArtifactsAsync(filePath: string, oldText: string, newText: string): Promise<DiffArtifacts>;
export function generateDiffArtifacts(filePath: string, oldText: string, newText: string): DiffArtifacts;
