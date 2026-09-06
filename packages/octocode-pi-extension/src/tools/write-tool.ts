/**
 * Write operations used by the public file tool.
 * Atomic writes record read-state for subsequent edit stale checks.
 */
import path from 'node:path';
import type { ToolCallResult } from '../types.js';
import { atomicWriteUtf8, recordFileReadStateFromContent, withFileMutationQueue } from './file-state.js';
import { peerWipNotice, markOwnWrite } from './peer-wip.js';

export function resolveWritePath(filePath: string, cwd = process.cwd()): string {
  return path.isAbsolute(filePath) ? filePath : path.resolve(cwd, filePath);
}

export function validateWriteParams(params: Record<string, unknown>): { path: string; content: string; reasoning: string } {
  const rawPath = params['path'];
  if (typeof rawPath !== 'string' || rawPath.trim().length === 0) {
    throw new Error('Write tool input is invalid. path must be a non-empty string.');
  }
  if (typeof params['content'] !== 'string') {
    throw new Error('Write tool input is invalid. content must be a string.');
  }
  if (typeof params['reasoning'] !== 'string' || params['reasoning'].trim().length === 0) {
    throw new Error('Write tool input is invalid. reasoning is required — provide a non-empty string explaining why this write is necessary.');
  }
  return { path: rawPath, content: params['content'], reasoning: params['reasoning'] };
}

/** Execute one path-guarded write after the caller has preflighted the batch. */
export async function commitWrite(
  requestPath: string,
  content: string,
  cwd: string,
  signal?: AbortSignal,
): Promise<ToolCallResult> {
  const absolutePath = resolveWritePath(requestPath, cwd);
  if (signal?.aborted) throw new Error('Operation aborted');
  const peerNotice = peerWipNotice(absolutePath, requestPath);

  await withFileMutationQueue(absolutePath, async () => {
    if (signal?.aborted) throw new Error('Operation aborted');
    await atomicWriteUtf8(absolutePath, content);
    if (signal?.aborted) throw new Error('Operation aborted');
    await recordFileReadStateFromContent(absolutePath, content);
    markOwnWrite(absolutePath);
  });

  return {
    content: [{
      type: 'text',
      text: `Successfully wrote ${Buffer.byteLength(content, 'utf8')} bytes to ${requestPath}${peerNotice}`,
    }],
    details: {
      operation: 'write',
      path: requestPath,
      absolutePath,
      bytes: Buffer.byteLength(content, 'utf8'),
    },
  };
}
