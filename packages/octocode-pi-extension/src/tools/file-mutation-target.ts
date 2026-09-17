import path from 'node:path';
import {
  NativeErrorCodes,
  nativeErrorCode,
  type FileSnapshot,
} from '@octocodeai/octocode-extension-rust';
import { assertPathAllowed, resolveCanonicalPath } from './path-guard.js';
import { snapshotNativeFile } from './native-files.js';
import { assertWellFormedText } from './file-text.js';

/** Native snapshot plus host path policy; external-writer CAS is not claimed. */
export interface FileMutationTarget {
  requestPath: string;
  absolutePath: string;
  canonicalPath: string;
  cwd: string;
  snapshot: FileSnapshot;
}

export interface FileMutationRecoveryV1 {
  tool: 'MCPTool';
  query: {
    queries: Array<{
      action: 'call';
      tool: 'localFetch';
      arguments: { queries: Array<{ path: string }> };
    }>;
  };
  why: string;
}

export class FileMutationConflictError extends Error {
  readonly code = 'file-mutation-conflict';
  readonly recovery: FileMutationRecoveryV1;

  constructor(message: string, absolutePath: string) {
    super(message);
    this.name = 'FileMutationConflictError';
    this.recovery = {
      tool: 'MCPTool',
      query: {
        queries: [{
          action: 'call',
          tool: 'localFetch',
          arguments: { queries: [{ path: absolutePath }] },
        }],
      },
      why: 'The file changed after preflight; inspect current bytes before preparing a new mutation.',
    };
  }
}

export function rethrowFileMutationConflict(
  error: unknown,
  target: Pick<FileMutationTarget, 'requestPath' | 'canonicalPath'>,
): never {
  if (error instanceof FileMutationConflictError) throw error;
  const detail = error instanceof Error ? error.message : String(error);
  const nodeCode = error && typeof error === 'object' && 'code' in error ? error.code : undefined;
  if (nativeErrorCode(error) === NativeErrorCodes.PRECONDITION_FAILED || nodeCode === 'EEXIST') {
    throw new FileMutationConflictError(detail, target.canonicalPath);
  }
  throw error;
}

export async function prepareFileMutationTarget(
  requestPath: string, cwd: string, allowMissing: boolean, includeContent = false,
): Promise<FileMutationTarget> {
  assertWellFormedText(requestPath, 'path');
  const absolutePath = path.resolve(cwd, requestPath);
  assertPathAllowed(absolutePath, cwd, 'file mutation');
  const canonicalPath = resolveCanonicalPath(absolutePath);
  const snapshot = await snapshotNativeFile(canonicalPath, includeContent);
  if (!allowMissing && !snapshot.exists) throw new Error(`File not found: ${requestPath}`);
  return { requestPath, absolutePath, canonicalPath, cwd, snapshot };
}

/** Revalidate host policy/aliases; Rust rechecks the full snapshot at commit. */
export function assertFileMutationTargetCurrent(target: FileMutationTarget): void {
  assertPathAllowed(target.absolutePath, target.cwd, 'file mutation');
  if (resolveCanonicalPath(target.absolutePath) !== target.canonicalPath) {
    throw new FileMutationConflictError(
      `${target.requestPath} changed after preflight. Re-read the file and retry.`,
      target.canonicalPath,
    );
  }
}
