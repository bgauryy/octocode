/**
 * Write operations used by the public file tool.
 * Atomic writes record read-state for subsequent edit stale checks.
 */
import type { ToolCallResult } from '../types.js';
import { resolveFilePath, withFileMutationQueue } from './file-state.js';
import { assertFileContentSize, replaceNativeFile } from './native-files.js';
import { createCommittedMutationReceipt, finishFileMutation } from './file-mutation-receipt.js';
import { assertWellFormedText } from './file-text.js';
import { prepareFileMutationTarget, assertFileMutationTargetCurrent, rethrowFileMutationConflict, type FileMutationTarget } from './file-mutation-target.js';

export function resolveWritePath(filePath: string, cwd = process.cwd()): string {
  return resolveFilePath(filePath, cwd);
}

export interface PreparedWrite {
  operation: 'write';
  target: FileMutationTarget;
  content: string;
  previousLines: number;
}

export async function prepareWrite(requestPath: string, content: string, cwd: string): Promise<PreparedWrite> {
  assertFileContentSize(content);
  const target = await prepareFileMutationTarget(requestPath, cwd, true, true);
  const previous = target.snapshot.content?.toString('utf8') ?? '';
  const previousLines = previous.length === 0 ? 0 : previous.endsWith('\n') ? previous.split('\n').length - 1 : previous.split('\n').length;
  delete target.snapshot.content;
  return { operation: 'write', target, content, previousLines };
}

export function validateWriteParams(params: Record<string, unknown>): { path: string; content: string } {
  const rawPath = params['path'];
  if (typeof rawPath !== 'string' || rawPath.trim().length === 0) {
    throw new Error('Write tool input is invalid. path must be a non-empty string.');
  }
  if (typeof params['content'] !== 'string') {
    throw new Error('Write tool input is invalid. content must be a string.');
  }
  assertWellFormedText(params['content'], 'content');
  return { path: rawPath, content: params['content'] };
}

/** Execute one path-guarded write after the caller has preflighted the batch. */
export async function commitWrite(
  prepared: PreparedWrite,
  signal?: AbortSignal,
): Promise<ToolCallResult> {
  const { target, content } = prepared;
  const { requestPath, canonicalPath: absolutePath } = target;
  if (signal?.aborted) throw new Error('Operation aborted');
  const created = !target.snapshot.exists;

  let committed: Awaited<ReturnType<typeof replaceNativeFile>>;
  let warnings: string[];
  try {
    ({ receipt: committed, warnings } = await withFileMutationQueue(absolutePath, async () => {
      if (signal?.aborted) throw new Error('Operation aborted');
      assertFileMutationTargetCurrent(target);
      const receipt = await replaceNativeFile(absolutePath, content, target.snapshot.version, signal);
      const nextWarnings = [...receipt.warnings, ...await finishFileMutation(absolutePath, content)];
      return { receipt, warnings: nextWarnings };
    }));
  } catch (error) {
    rethrowFileMutationConflict(error, target);
  }

  return {
    content: [{
      type: 'text',
      text: `Successfully wrote ${Buffer.byteLength(content, 'utf8')} bytes to ${requestPath}${warnings.length ? `\n${warnings.join('\n')}` : ''}`,
    }],
    details: {
      operation: 'write',
      committed: true,
      durable: committed.durable,
      ...(warnings.length ? { warnings } : {}),
      created,
      path: requestPath,
      absolutePath: target.absolutePath,
      canonicalPath: absolutePath,
      bytes: Buffer.byteLength(content, 'utf8'),
      mutation: createCommittedMutationReceipt({ snapshot: target.snapshot, nextContent: content, previousLines: prepared.previousLines }),
    },
  };
}
