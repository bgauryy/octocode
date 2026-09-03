/** First-class Pi UI for the package-owned external-agent memory contract. */
import {
  executeExternalMemoryAction,
  EXTERNAL_MEMORY_ACTIONS,
  EXTERNAL_MEMORY_RECALL_MODES,
  validateExternalMemoryParams,
  type ExternalMemoryParams,
  type ExternalMemoryResult,
} from '@octocodeai/octocode-awareness';
import type { ToolDefinition, ToolCallResult, PiTheme, PiContext } from '../types.js';
import type { registerUniqueTool } from './octocode-tools.js';
import { buildToolView } from './render-helpers.js';
import { buildQueryEnvelopeSchema, executeQueryBatch } from './query-envelope.js';

type TypeBoxBuilder = (typeof import('typebox'))['Type'];
type RegisterFn = typeof registerUniqueTool;
export type MemoryActionRunner = (input: { workspace: string; params: ExternalMemoryParams }) => ExternalMemoryResult | Promise<ExternalMemoryResult>;

const defaultRunner: MemoryActionRunner = executeExternalMemoryAction;
let runner: MemoryActionRunner = defaultRunner;

export function setMemoryActionRunnerForTests(fn: MemoryActionRunner | null): void {
  runner = fn ?? defaultRunner;
}

function errorResult(text: string): ToolCallResult {
  return { content: [{ type: 'text', text }], isError: true } as unknown as ToolCallResult;
}

function toToolResult(result: ExternalMemoryResult): ToolCallResult {
  const payload = result.action === 'suggest'
    ? result.candidate
    : result.action === 'review'
      ? { result: result.result, candidates: result.candidates }
      : result.result;
  return {
    content: [{ type: 'text', text: payload === undefined ? result.summary : `${result.summary}\n${JSON.stringify(payload)}` }],
    details: result,
  } as unknown as ToolCallResult;
}

export function registerMemoryTool(
  pi: { registerTool?(def: ToolDefinition): void },
  Type: TypeBoxBuilder,
  registeredToolNames: Set<string>,
  registerFn: RegisterFn,
): void {
  registerFn(pi, registeredToolNames, {
    name: 'memory',
    label: 'Memory',
    description: [
      'Recall, record, review, suggest, or forget durable Awareness memory in the shared SQLite store.',
      'Recall results are leads: re-verify against current source and tests. Suggest validates and shapes a candidate without storing it; review never mutates.',
      'Record only verified reusable learnings, gotchas, or decisions. Never store secrets, raw logs, routine status, or facts already owned by source/docs.',
    ].join('\n'),
    promptSnippet: 'Recall or maintain verified durable Awareness memory',
    promptGuidelines: [
      'Recall only when prior learning may change the approach; re-verify every returned lead.',
      'Record after verification with evidence and context. Use suggest when uncertain; forget only a clearly obsolete id.',
    ],
    parameters: buildQueryEnvelopeSchema(Type, Type.Object({
      action: Type.Unsafe({ type: 'string', enum: EXTERNAL_MEMORY_ACTIONS, description: EXTERNAL_MEMORY_ACTIONS.join('|') }),
      query: Type.Optional(Type.String({ description: 'recall/review search text.' })),
      mode: Type.Optional(Type.Unsafe({ type: 'string', enum: EXTERNAL_MEMORY_RECALL_MODES, description: EXTERNAL_MEMORY_RECALL_MODES.join('|') })),
      label: Type.Optional(Type.String({ description: 'record label or recall/review filter.' })),
      observation: Type.Optional(Type.String({ description: 'verified reusable learning.' })),
      importance: Type.Optional(Type.Integer({ minimum: 1, maximum: 10 })),
      taskContext: Type.Optional(Type.String({ description: 'when the learning matters.' })),
      source: Type.Optional(Type.String({ description: 'file:line or observed check evidence.' })),
      tags: Type.Optional(Type.Array(Type.String(), { description: 'searchable package/area tags.' })),
      changedFiles: Type.Optional(Type.Array(Type.String(), { description: 'suggest-only tag hints.' })),
      limit: Type.Optional(Type.Integer({ minimum: 1, maximum: 100 })),
      memoryId: Type.Optional(Type.String({ description: 'forget target id.' })),
    }, { additionalProperties: false }), {
      reasoningDescription: 'Concise evidence-based reason for this memory operation.',
    }),

    async execute(id: string, raw: Record<string, unknown>, signal, onUpdate, ctx?: PiContext): Promise<ToolCallResult> {
      const queries = Array.isArray(raw.queries) ? raw.queries as ExternalMemoryParams[] : [];
      if (queries.length === 1) {
        try {
          validateExternalMemoryParams(queries[0]!);
        } catch (error) {
          return errorResult(`[memory] ${error instanceof Error ? error.message : String(error)}`);
        }
      }
      return executeQueryBatch({
        toolCallId: id,
        raw,
        signal,
        onUpdate: typeof onUpdate === 'function' ? onUpdate as (update: ToolCallResult) => void : undefined,
        ctx,
        passthroughSingle: true,
        preflight: queries.length > 1
          ? (query) => validateExternalMemoryParams(query as unknown as ExternalMemoryParams)
          : undefined,
        async execute(query) {
          try {
            return toToolResult(await runner({
              workspace: ctx?.cwd ?? process.cwd(),
              params: query as unknown as ExternalMemoryParams,
            }));
          } catch (error) {
            return errorResult(`[memory] ${error instanceof Error ? error.message : String(error)}`);
          }
        },
      });
    },

    renderCall(raw: unknown, theme?: PiTheme) {
      const envelope = raw && typeof raw === 'object' ? raw as Record<string, unknown> : {};
      const queries = Array.isArray(envelope['queries']) ? envelope['queries'] as ExternalMemoryParams[] : [];
      const params = queries[0];
      const action = params?.action ?? 'recall';
      const hint = params?.query ? `"${params.query}"` : params?.label ? `[${params.label}]` : params?.memoryId ?? '';
      return buildToolView({ name: 'memory', state: 'request', segments: [{ text: action, token: 'bright' }, ...(hint ? [{ text: hint, token: 'dim' as const }] : [])] }, theme);
    },

    renderResult(result: ToolCallResult, _opts: unknown, theme?: PiTheme) {
      const ok = !result.isError;
      const first = ((result.content?.[0] as { text?: string } | undefined)?.text ?? '').split('\n')[0] || 'memory';
      return buildToolView({ name: 'memory', state: ok ? 'success' : 'error', segments: [{ text: first, token: ok ? 'dim' : 'error' }] }, theme);
    },
  });
}
