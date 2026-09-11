import path from 'node:path';
import {
  NativeErrorCodes,
  nativeErrorCode,
  type FileSnapshot,
} from '@octocodeai/octocode-extension-rust';
import type { ToolCallResult, ToolDefinition, PiTheme } from '../types.js';
import { CLI_STATUS_TEXT } from '../tui/cli-design.js';
import { buildQueryCallBlocks, buildToolView } from './render-helpers.js';
import { assertPathAllowed, canonicalPathKey, resolveCanonicalPath } from './path-guard.js';
import {
  resolveFilePath,
  withFileMutationQueue,
} from './file-state.js';
import { peerWipNotice } from './peer-wip.js';
import { countMutationLines, createCommittedMutationReceipt, finishFileMutation } from './file-mutation-receipt.js';
import { deleteNativeFile, snapshotNativeFile } from './native-files.js';
import { assertWellFormedText } from './file-text.js';
import { FileMutationConflictError, rethrowFileMutationConflict } from './file-mutation-target.js';
import {
  commitPreparedEdit,
  prepareEdit,
  renderEditResult,
  validateEditQuery,
  type PreparedEdit,
} from './edit-tool.js';
import { commitWrite, prepareWrite, validateWriteParams, type PreparedWrite } from './write-tool.js';
import { DIRECT_TOOL_DESCRIPTIONS, type registerUniqueTool } from './octocode-tools.js';
import { buildQueryEnvelopeSchema, executeQueryBatch, QUERY_BATCH_MAX_ITEMS, type QueryRecord } from './query-envelope.js';

import { z } from 'zod';
type RegisterFn = typeof registerUniqueTool;
type FileOperation = 'edit' | 'write' | 'delete';

interface PreparedDelete {
  operation: 'delete';
  path: string;
  absolutePath: string;
  canonicalPath: string;
  snapshot: FileSnapshot;
  previousLines: number;
}

interface PreparedEditOperation {
  operation: 'edit';
  edit: PreparedEdit;
}

type PreparedFileOperation = PreparedWrite | PreparedDelete | PreparedEditOperation;

const FILE_TOOL_DISPLAY_NAME = 'file (Octocode)';

function assertOnlyFields(query: QueryRecord, allowed: readonly string[], operation: FileOperation): void {
  const allowedSet = new Set(['reasoning', 'type', ...allowed]);
  const extra = Object.keys(query).filter((key) => !allowedSet.has(key));
  if (extra.length > 0) throw new Error(`${operation} does not accept ${extra.join(', ')} — valid fields for ${operation}: type, reasoning, ${allowed.join(', ')}.`);
}

function validateBase(query: QueryRecord): { operation: FileOperation; path: string } {
  const operation = query['type'];
  if (operation !== 'edit' && operation !== 'write' && operation !== 'delete') {
    throw new Error('file type must be edit, write, or delete.');
  }
  const path = query['path'];
  if (typeof path !== 'string' || path.trim().length === 0) {
    throw new Error(`${operation} requires a non-empty path.`);
  }
  assertWellFormedText(path, 'path');
  return { operation, path };
}

async function prepareOperation(query: QueryRecord, index: number, cwd: string): Promise<PreparedFileOperation> {
  const { operation, path } = validateBase(query);
  if (operation === 'write') {
    assertOnlyFields(query, ['path', 'content'], operation);
    const validated = validateWriteParams(query);
    return prepareWrite(validated.path, validated.content, cwd);
  }

  if (operation === 'edit') {
    assertOnlyFields(query, ['path', 'edits', 'requireRecentRead'], operation);
    if (!Array.isArray(query['edits']) || query['edits'].length === 0) {
      throw new Error('edit requires a non-empty edits array.');
    }
    fileItemSchema.parse(query);
    const edits = query['edits'].map((value) => {
      if (!value || typeof value !== 'object' || Array.isArray(value)) return value;
      return { ...(value as Record<string, unknown>), reasoning: query.reasoning };
    });
    const editQuery = validateEditQuery({
      path,
      edits,
      ...(query['requireRecentRead'] === undefined ? {} : { requireRecentRead: query['requireRecentRead'] }),
    }, index);
    return { operation, edit: await prepareEdit(editQuery, cwd, false) };
  }

  assertOnlyFields(query, ['path'], operation);
  const absolutePath = resolveFilePath(path, cwd);
  assertPathAllowed(absolutePath, cwd, 'file delete');
  const canonicalPath = canonicalDeletePath(absolutePath);
  const snapshot = await snapshotNativeFile(canonicalPath, true, true).catch((error: unknown) => {
    if (nativeErrorCode(error) === NativeErrorCodes.NOT_REGULAR_FILE) {
      throw new Error(`delete supports files and symbolic links, not directories or other nonregular entries: ${path}`);
    }
    throw error;
  });
  if (!snapshot.exists) throw new Error(`File does not exist: ${path}`);
  const previousLines = snapshot.content ? countMutationLines(snapshot.content.toString('utf8')) : 0;
  delete snapshot.content;
  return { operation, path, absolutePath, canonicalPath, snapshot, previousLines };
}

/** Canonicalize the parent only: delete removes the link itself. */
function canonicalDeletePath(absolutePath: string): string {
  return path.join(resolveCanonicalPath(path.dirname(absolutePath)), path.basename(absolutePath));
}

async function commitDelete(prepared: PreparedDelete, cwd: string, signal?: AbortSignal): Promise<ToolCallResult> {
  if (signal?.aborted) throw new Error('Operation aborted');
  const peerNotice = peerWipNotice(prepared.absolutePath, prepared.path);
  let committed: Awaited<ReturnType<typeof deleteNativeFile>>;
  let warnings: string[];
  try {
    ({ receipt: committed, warnings } = await withFileMutationQueue(prepared.absolutePath, async () => {
      if (signal?.aborted) throw new Error('Operation aborted');
      assertPathAllowed(prepared.absolutePath, cwd, 'file delete');
      if (canonicalDeletePath(prepared.absolutePath) !== prepared.canonicalPath) {
        throw new FileMutationConflictError(
          `${prepared.path} changed after delete preflight. Re-inspect it and retry.`,
          prepared.canonicalPath,
        );
      }
      const receipt = await deleteNativeFile(prepared.canonicalPath, prepared.snapshot.version, signal);
      const nextWarnings = [...receipt.warnings, ...await finishFileMutation(prepared.absolutePath)];
      return { receipt, warnings: nextWarnings };
    }));
  } catch (error) {
    rethrowFileMutationConflict(error, { requestPath: prepared.path, canonicalPath: prepared.canonicalPath });
  }
  return {
    content: [{ type: 'text', text: `Deleted ${prepared.path}.${peerNotice}${warnings.length ? `\n${warnings.join('\n')}` : ''}` }],
    details: {
      operation: 'delete',
      committed: true,
      durable: committed.durable,
      path: prepared.path,
      absolutePath: prepared.absolutePath,
      mutation: createCommittedMutationReceipt({ snapshot: prepared.snapshot, previousLines: prepared.previousLines }),
      ...(warnings.length ? { warnings } : {}),
    },
  };
}

const fileEditOperationSchema = z.strictObject({
  oldText: z.string().optional().describe('Current text; required except for lineRange.'),
  newText: z.string().describe('Replacement text.'),
  replaceAll: z.boolean().optional().describe('Replace every match; default false.'),
  matchMode: z.enum(['exact', 'normalized', 'lineRange']).optional().describe('Match strategy; default exact.'),
  startLine: z.number().int().min(1).optional().describe('First line for lineRange.'),
  endLine: z.number().int().min(1).optional().describe('Inclusive last line for lineRange.'),
});

const fileItemSchema = z.looseObject({
  type: z.enum(['edit', 'write', 'delete']).describe('Mutation operation.'),
  path: z.string().min(1).describe('Target file path.'),
  content: z.string().optional().describe('Complete content for write.'),
  edits: z.array(fileEditOperationSchema).min(1).optional().describe('Targeted replacements for edit.'),
  requireRecentRead: z.boolean().optional().describe('Require a fresh recorded read before edit.'),
});

export function registerFileTool(
  pi: { registerTool?(def: ToolDefinition): void },
  registeredToolNames: Set<string>,
  registerFn: RegisterFn,
): void {
  registerFn(pi, registeredToolNames, {
    name: 'file',
    label: FILE_TOOL_DISPLAY_NAME,
    description: DIRECT_TOOL_DESCRIPTIONS.file!,
    promptSnippet: 'Apply scoped file edits, full writes, or deletions.',
    promptGuidelines: [
      'Use type:"edit" for targeted replacements, type:"write" for new files or intentional full rewrites, and type:"delete" only when removal is explicitly in scope.',
      'After reasoning and type, write accepts path+content; delete accepts path; edit accepts path+edits+requireRecentRead. Extra fields such as confirm, force, or dryRun fail preflight.',
      'Read and understand existing files before edit/delete. Use exact oldText by default; normalized or lineRange matching is opt-in.',
      'For requireRecentRead or a lineRange edit without oldText, read through MCPTool localFetch first; shell reads do not refresh the stale-edit guard.',
      'Keep replacements bounded with the smallest unique anchor, and split large mutations across separate calls before the model output limit.',
      'Batch edits to one path in a single query. All queries are preflighted before mutation; duplicate target paths are rejected.',
    ],
    parameters: buildQueryEnvelopeSchema(fileItemSchema, {
      reasoningDescription: 'Why this file mutation is necessary.',
    }),
    async execute(toolCallId, params, signal, onUpdate, ctx): Promise<ToolCallResult> {
      const cwd = ctx?.cwd ?? process.cwd();
      const rawQueries = Array.isArray(params['queries']) ? params['queries'] as Array<Record<string, unknown>> : [];
      for (const query of rawQueries) {
        if (typeof query?.['path'] === 'string') assertWellFormedText(query['path'], 'path');
      }
      const resolved = rawQueries
        .map((query) => typeof query?.['path'] === 'string' ? resolveCanonicalPath(resolveFilePath(query['path'], cwd)) : '')
        .filter(Boolean);
      if (new Set(resolved.map(value => canonicalPathKey(value))).size !== resolved.length) {
        throw new Error('file queries must not contain duplicate target paths.');
      }

      const prepared = new Map<number, PreparedFileOperation>();
      return executeQueryBatch({
        toolCallId,
        raw: params,
        signal,
        onUpdate: typeof onUpdate === 'function' ? onUpdate as (update: ToolCallResult) => void : undefined,
        ctx,
        passthroughSingle: true,
        maxItems: QUERY_BATCH_MAX_ITEMS,
        async preflight(query, index) {
          prepared.set(index, await prepareOperation(query, index, cwd));
        },
        async execute(_query, index) {
          const operation = prepared.get(index)!;
          if (operation.operation === 'edit') return commitPreparedEdit(operation.edit, signal);
          if (operation.operation === 'write') return commitWrite(operation, signal);
          return commitDelete(operation, cwd, signal);
        },
        summarize(result) {
          const details = result.details as { operation?: string; path?: string } | undefined;
          return `${details?.operation ?? 'file'} ${details?.path ?? ''}`.trim();
        },
      });
    },
    renderCall(args: unknown, theme?: PiTheme) {
      return buildQueryCallBlocks(args, theme, (singleArgs) => {
        const queries = Array.isArray(singleArgs['queries'])
          ? singleArgs['queries'] as Array<Record<string, unknown>>
          : [];
        const first = queries[0] ?? {};
        const operation = typeof first['type'] === 'string' ? first['type'] : 'mutate';
        const filePath = typeof first['path'] === 'string' ? first['path'] : '(missing path)';
        return buildToolView({ name: FILE_TOOL_DISPLAY_NAME, state: 'request', segments: [{ text: operation, token: 'bright' }, { text: filePath, token: 'path' }] }, theme);
      });
    },
    renderResult(result: ToolCallResult, opts: { isPartial?: boolean; expanded?: boolean }, theme?: PiTheme) {
      // Partial: spinner + tool name while the batch is still executing.
      if (opts.isPartial) {
        return buildToolView(() => ({ name: FILE_TOOL_DISPLAY_NAME, state: 'running', status: CLI_STATUS_TEXT.editing }), theme);
      }

      const details = result.details as { operation?: string; path?: string; bytes?: number } | undefined;
      const op = details?.operation;

      // Edit operations: delegate to the shared edit renderer — shows diff + reasoning.
      if (op === 'edit') return renderEditResult(result, opts, theme, FILE_TOOL_DISPLAY_NAME);

      // Write / Delete: icon + tool name, then optional «op · path» and a summary line.
      const ok = !result.isError;
      const hasPath = typeof details?.path === 'string';
      // «op · /the/path» — separator only when BOTH op and path are present so we
      // never render a dangling «write ·» when the detail object has no path key.
      const summary =
        op === 'write' && typeof details?.bytes === 'number'
          ? `${details.bytes} bytes written`
          : result.content.find((item) => item.type === 'text')?.text?.split('\n')[0] ?? '';
      return buildToolView({
        name: FILE_TOOL_DISPLAY_NAME,
        state: ok ? 'success' : 'error',
        segments: [
          ...(op ? [{ text: op, token: 'bright' as const }] : []),
          ...(hasPath ? [{ text: details!.path!, token: 'path' as const }] : []),
        ],
        body: summary ? [{ text: summary, token: ok ? 'muted' : 'error' }] : [],
      }, theme);
    },
  });
}
