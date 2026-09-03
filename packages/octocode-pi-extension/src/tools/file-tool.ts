import { lstat, unlink } from 'node:fs/promises';
import type { Stats } from 'node:fs';
import type { TSchema, ToolCallResult, ToolDefinition, PiTheme } from '../types.js';
import { CLI_STATUS_TEXT } from '../tui/cli-design.js';
import { buildQueryCallBlocks, buildToolView } from './render-helpers.js';
import { assertPathAllowed } from './path-guard.js';
import {
  forgetFileReadState,
  resolveFilePath,
  withFileMutationQueue,
} from './file-state.js';
import { markOwnWrite, peerWipNotice } from './peer-wip.js';
import {
  commitPreparedEdit,
  prepareEdit,
  renderEditResult,
  validateEditQuery,
  type PreparedEdit,
} from './edit-tool.js';
import { commitWrite, resolveWritePath, validateWriteParams } from './write-tool.js';
import type { registerUniqueTool } from './octocode-tools.js';
import { buildQueryEnvelopeSchema, executeQueryBatch, QUERY_BATCH_MAX_ITEMS, type QueryRecord } from './query-envelope.js';

type TypeBoxBuilder = (typeof import('typebox'))['Type'];
type RegisterFn = typeof registerUniqueTool;
type FileOperation = 'edit' | 'write' | 'delete';

interface PreparedWrite {
  operation: 'write';
  path: string;
  content: string;
}

interface DeleteSnapshot {
  dev: number;
  ino: number;
  mode: number;
  size: number;
  mtimeMs: number;
  ctimeMs: number;
}

interface PreparedDelete {
  operation: 'delete';
  path: string;
  absolutePath: string;
  snapshot: DeleteSnapshot;
}

interface PreparedEditOperation {
  operation: 'edit';
  edit: PreparedEdit;
}

type PreparedFileOperation = PreparedWrite | PreparedDelete | PreparedEditOperation;

const FILE_TOOL_DISPLAY_NAME = 'file (Octocode)';

function snapshot(stats: Stats): DeleteSnapshot {
  return {
    dev: stats.dev,
    ino: stats.ino,
    mode: stats.mode,
    size: stats.size,
    mtimeMs: stats.mtimeMs,
    ctimeMs: stats.ctimeMs,
  };
}

function sameSnapshot(left: DeleteSnapshot, right: DeleteSnapshot): boolean {
  return left.dev === right.dev
    && left.ino === right.ino
    && left.mode === right.mode
    && left.size === right.size
    && left.mtimeMs === right.mtimeMs
    && left.ctimeMs === right.ctimeMs;
}

function assertOnlyFields(query: QueryRecord, allowed: readonly string[], operation: FileOperation): void {
  const allowedSet = new Set(['reasoning', 'type', ...allowed]);
  const extra = Object.keys(query).filter((key) => !allowedSet.has(key));
  if (extra.length > 0) throw new Error(`${operation} does not accept ${extra.join(', ')}.`);
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
  return { operation, path };
}

async function prepareOperation(query: QueryRecord, index: number, cwd: string): Promise<PreparedFileOperation> {
  const { operation, path } = validateBase(query);
  if (operation === 'write') {
    assertOnlyFields(query, ['path', 'content'], operation);
    const validated = validateWriteParams(query);
    assertPathAllowed(resolveWritePath(validated.path, cwd), cwd, 'file write');
    return { operation, path: validated.path, content: validated.content };
  }

  if (operation === 'edit') {
    assertOnlyFields(query, ['path', 'edits', 'requireRecentRead'], operation);
    if (!Array.isArray(query['edits']) || query['edits'].length === 0) {
      throw new Error('edit requires a non-empty edits array.');
    }
    const edits = query['edits'].map((value) => {
      if (!value || typeof value !== 'object' || Array.isArray(value)) return value;
      return { ...(value as Record<string, unknown>), reasoning: query.reasoning };
    });
    const editQuery = validateEditQuery({
      path,
      edits,
      ...(query['requireRecentRead'] === true ? { requireRecentRead: true } : {}),
    }, index);
    return { operation, edit: await prepareEdit(editQuery, cwd, false) };
  }

  assertOnlyFields(query, ['path'], operation);
  const absolutePath = resolveFilePath(path, cwd);
  assertPathAllowed(absolutePath, cwd, 'file delete');
  const stats = await lstat(absolutePath);
  if (!stats.isFile() && !stats.isSymbolicLink()) {
    throw new Error(`delete supports files and symbolic links, not directories: ${path}`);
  }
  return { operation, path, absolutePath, snapshot: snapshot(stats) };
}

async function commitDelete(prepared: PreparedDelete, cwd: string, signal?: AbortSignal): Promise<ToolCallResult> {
  if (signal?.aborted) throw new Error('Operation aborted');
  const peerNotice = peerWipNotice(prepared.absolutePath, prepared.path);
  await withFileMutationQueue(prepared.absolutePath, async () => {
    if (signal?.aborted) throw new Error('Operation aborted');
    const current = snapshot(await lstat(prepared.absolutePath));
    if (!sameSnapshot(prepared.snapshot, current)) {
      throw new Error(`${prepared.path} changed after delete preflight. Re-inspect it and retry.`);
    }
    await unlink(prepared.absolutePath);
    forgetFileReadState(prepared.absolutePath, cwd);
    markOwnWrite(prepared.absolutePath);
  });
  return {
    content: [{ type: 'text', text: `Deleted ${prepared.path}.${peerNotice}` }],
    details: { operation: 'delete', path: prepared.path, absolutePath: prepared.absolutePath },
  };
}

function buildParameters(Type: TypeBoxBuilder): TSchema {
  const editOperation = Type.Object({
    oldText: Type.Optional(Type.String({ description: 'Current text; required except for lineRange.' })),
    newText: Type.String({ description: 'Replacement text.' }),
    replaceAll: Type.Optional(Type.Boolean({ description: 'Replace every match; default false.' })),
    matchMode: Type.Optional(Type.Unsafe({ type: 'string', enum: ['exact', 'normalized', 'lineRange'], description: 'Match strategy; default exact.' })),
    startLine: Type.Optional(Type.Integer({ minimum: 1, description: 'First line for lineRange.' })),
    endLine: Type.Optional(Type.Integer({ minimum: 1, description: 'Inclusive last line for lineRange.' })),
  }, { additionalProperties: false });
  const item = Type.Object({
    type: Type.Unsafe({ type: 'string', enum: ['edit', 'write', 'delete'], description: 'Mutation operation.' }),
    path: Type.String({ minLength: 1, description: 'Target file path.' }),
    content: Type.Optional(Type.String({ description: 'Complete content for write.' })),
    edits: Type.Optional(Type.Array(editOperation, { minItems: 1, description: 'Targeted replacements for edit.' })),
    requireRecentRead: Type.Optional(Type.Boolean({ description: 'Require a fresh recorded read before edit.' })),
  }, {
    additionalProperties: false,
    oneOf: [
      { title: 'edit', properties: { type: { const: 'edit' } }, required: ['type', 'path', 'edits'] },
      { title: 'write', properties: { type: { const: 'write' } }, required: ['type', 'path', 'content'] },
      { title: 'delete', properties: { type: { const: 'delete' } }, required: ['type', 'path'] },
    ],
  });
  return buildQueryEnvelopeSchema(Type, item as TSchema, {
    reasoningDescription: 'Why this file mutation is necessary.',
  });
}

export function registerFileTool(
  pi: { registerTool?(def: ToolDefinition): void },
  Type: TypeBoxBuilder,
  registeredToolNames: Set<string>,
  registerFn: RegisterFn,
): void {
  registerFn(pi, registeredToolNames, {
    name: 'file',
    label: FILE_TOOL_DISPLAY_NAME,
    description: 'Create, edit, or delete files through one guarded mutation boundary. edit uses stale/lost-update checks and diffs; write is atomic; delete rejects directories and rechecks the target before unlinking.',
    promptSnippet: 'Create, edit, or delete files through one guarded mutation tool.',
    promptGuidelines: [
      'Use type:"edit" for targeted replacements, type:"write" for new files or intentional full rewrites, and type:"delete" only when removal is explicitly in scope.',
      'Read and understand existing files before edit/delete. Use exact oldText by default; normalized or lineRange matching is opt-in.',
      'Each query has one concise reasoning field. Mixed batches are fully preflighted before the first mutation and reject duplicate target paths.',
      'Do not use bash redirection, sed, or rm for ordinary file mutations when file is available.',
    ],
    parameters: buildParameters(Type),
    async execute(toolCallId, params, signal, onUpdate, ctx): Promise<ToolCallResult> {
      const cwd = ctx?.cwd ?? process.cwd();
      const rawQueries = Array.isArray(params['queries']) ? params['queries'] as Array<Record<string, unknown>> : [];
      const resolved = rawQueries
        .map((query) => typeof query?.['path'] === 'string' ? resolveFilePath(query['path'], cwd) : '')
        .filter(Boolean);
      if (new Set(resolved).size !== resolved.length) {
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
          if (operation.operation === 'write') return commitWrite(operation.path, operation.content, cwd, signal);
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
