/**
 * Octocode `write` — same-name override of Pi's built-in write.
 * Adds path-guard (home + ALLOWED_PATHS + cwd/tmp) and records read-state
 * so a subsequent `edit` stale-check can see the fresh bytes.
 */
import path from 'node:path';
import type { TSchema, ToolCallResult, ToolDefinition, PiTheme } from '../types.js';
import { buildToolView } from './render-helpers.js';
import { assertPathAllowed } from './path-guard.js';
import { atomicWriteUtf8, recordFileReadStateFromContent, withFileMutationQueue } from './file-state.js';
import { peerWipNotice, markOwnWrite } from './peer-wip.js';
import type { registerUniqueTool } from './octocode-tools.js';
import { buildQueryEnvelopeSchema, executeQueryBatch } from './query-envelope.js';

type TypeBoxBuilder = (typeof import('typebox'))['Type'];
type RegisterFn = typeof registerUniqueTool;

const WRITE_TOOL_DISPLAY_NAME = 'write (Octocode)';

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

export function registerWriteTool(
  pi: { registerTool?(def: ToolDefinition): void },
  Type: TypeBoxBuilder,
  registeredToolNames: Set<string>,
  registerFn: RegisterFn,
): void {
  const querySchema = Type.Object(
    {
      path: Type.String({ description: 'Path to the file to write (relative or absolute).' }),
      content: Type.String({ description: 'Content to write to the file.' }),
    },
    { additionalProperties: false },
  ) as TSchema;
  const parameters = buildQueryEnvelopeSchema(Type, querySchema, {
    reasoningDescription: 'Concise reason this file create or overwrite is necessary.',
    allowParallel: false,
  });

  registerFn(pi, registeredToolNames, {
    name: 'write',
    label: 'write (Octocode)',
    description:
      'Octocode custom write tool. Pass one or more ordered writes in queries; each query requires concise reasoning. queryRunType is sequential-only: writes always run one-by-one in source order, never in parallel. Replaces Pi built-in write with the same create/overwrite semantics plus Octocode path-guard (working directory, home, OS temp, ALLOWED_PATHS) and post-write read-state recording for the edit stale-check. Batches are preflighted, non-transactional, and stop on the first runtime failure. Prefer edit for surgical changes to existing files.',
    promptSnippet: 'Create or overwrite files with Octocode path-guard.',
    promptGuidelines: [
      'Octocode custom write replaces Pi built-in write; use write only for new files or intentional full rewrites.',
      'Prefer the edit tool for targeted replacements in existing files — write overwrites without an oldText match guard.',
      'Paths must stay inside the working directory, home directory, OS temp dir, or ALLOWED_PATHS.',
      'Do not use bash/cat redirection for ordinary file creates when write is available.',
    ],
    parameters,
    async execute(
      toolCallId: string,
      params: Record<string, unknown>,
      signal?: AbortSignal,
      onUpdate?: unknown,
      ctx?: { cwd?: string },
    ): Promise<ToolCallResult> {
      const cwd = ctx?.cwd ?? process.cwd();
      return executeQueryBatch({
        toolCallId,
        raw: params,
        signal,
        onUpdate: typeof onUpdate === 'function' ? onUpdate as (update: ToolCallResult) => void : undefined,
        ctx,
        passthroughSingle: true,
        preflight(query) {
          const { path: requestPath } = validateWriteParams(query);
          assertPathAllowed(resolveWritePath(requestPath, cwd), cwd, 'write');
        },
        async execute(query) {
          const { path: requestPath, content } = validateWriteParams(query);
          return commitWrite(requestPath, content, cwd, signal);
        },
      });
    },
    renderCall(args: unknown, theme?: PiTheme) {
      const envelope = args && typeof args === 'object' ? (args as Record<string, unknown>) : {};
      const queries = Array.isArray(envelope['queries']) ? envelope['queries'] as Record<string, unknown>[] : [];
      const input = queries[0] ?? {};
      const filePath = typeof input['path'] === 'string' ? input['path'] : '(missing path)';
      const content = typeof input['content'] === 'string' ? input['content'] : '';
      const lines = content.length === 0 ? 0 : content.split('\n').length;
      return buildToolView({
        name: WRITE_TOOL_DISPLAY_NAME,
        state: 'request',
        segments: [{ text: filePath, token: 'path' }, { text: `${lines} line${lines === 1 ? '' : 's'}`, token: 'count' }],
      }, theme);
    },
    renderResult(result: ToolCallResult, opts: { expanded?: boolean; isPartial?: boolean }, theme?: PiTheme) {
      if (opts.isPartial) {
        return buildToolView(() => ({ name: WRITE_TOOL_DISPLAY_NAME, state: 'running', status: 'writing…' }), theme);
      }
      if (!result.isError) {
        const batch = (result.details ?? {}) as { results?: unknown[] };
        if (Array.isArray(batch.results)) {
          return buildToolView({ name: WRITE_TOOL_DISPLAY_NAME, state: 'success', segments: [{ text: `${batch.results.length} writes`, token: 'count' }] }, theme);
        }
        // Result row shows WHAT was written: path + size (the model's text line
        // says the same thing; the user should not have to expand to see it).
        const d = (result.details ?? {}) as { path?: string; bytes?: number };
        return buildToolView({
          name: WRITE_TOOL_DISPLAY_NAME,
          state: 'success',
          segments: [
            ...(d.path ? [{ text: d.path, token: 'path' as const }] : []),
            ...(typeof d.bytes === 'number' ? [{ text: `${d.bytes} bytes`, token: 'count' as const }] : []),
          ],
        }, theme);
      }
      const text = result.content.find((c) => c.type === 'text')?.text ?? 'write failed';
      return buildToolView({ name: WRITE_TOOL_DISPLAY_NAME, state: 'error', segments: [{ text, token: 'error' }] }, theme);
    },
  });
}
