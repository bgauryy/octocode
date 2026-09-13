import {
  ROUTINE_AWARENESS_OPERATIONS,
  getAwarenessOperationDescriptor,
  type AwarenessExecutableCall,
} from '@octocodeai/octocode-awareness';
import { z } from 'zod';
import type { PiContext, PiInstance, PiTheme, ToolCallResult } from '../types.js';
import { truncateToWidth } from '../tui/width.js';
import { paint } from '../tui/palette.js';
import { DIRECT_TOOL_DESCRIPTIONS, type registerUniqueTool } from './octocode-tools.js';
import { buildQueryEnvelopeSchema, executeQueryBatch } from './query-envelope.js';
import { awarenessWriteReceipt } from './awareness-output.js';
import { buildAwarenessContext } from './awareness-context.js';
import { assertPersistentAwarenessEnabled } from './storage-policy.js';
import { requestApproval } from './approval.js';
import {
  runAwarenessOperation,
  type AwarenessOperationRunner,
} from './awareness-operation-runner.js';
import { makeComponentRenderer } from './render-helpers.js';
import {
  AWARENESS_OUTPUT_MAX_CHARS,
  boundedOutput,
  nativeOperationContinuations,
  record,
  result,
  routineApproval,
  validateRoutineParams,
} from './awareness-tool-protocol.js';

type RegisterFn = typeof registerUniqueTool;
const NATIVE_HISTORY_READ_CHUNK_BYTES = 6 * 1024;

function validateOperationQuery(query: Record<string, unknown>): void {
  const operation = String(query['operation'] ?? '').trim();
  const descriptor = getAwarenessOperationDescriptor(operation);
  if (!descriptor) throw new Error(`Unknown Awareness operation: "${operation}"`);
  const obsoleteField = ['action', 'command', 'noun', 'all', 'page', 'pageSize']
    .find(field => query[field] !== undefined);
  if (obsoleteField) throw new Error(`${obsoleteField} is not part of the canonical Awareness surface`);
  if (query['describe'] === true) {
    if (query['params'] !== undefined) throw new Error('Awareness describe cannot include params; describe reads only the operation schema');
    return;
  }
  const params = record(query['params']) ?? {};
  try {
    validateRoutineParams(descriptor, params);
  } catch (error) {
    throw new Error(`Invalid parameters for Awareness ${operation}: ${error instanceof Error ? error.message : String(error)}`);
  }
}

async function callOperation(
  runner: AwarenessOperationRunner,
  query: Record<string, unknown>,
  signal?: AbortSignal,
  ctx?: PiContext,
): Promise<ToolCallResult> {
  const operation = String(query['operation'] ?? '').trim();
  const descriptor = getAwarenessOperationDescriptor(operation);
  if (!descriptor)
    return result(`Unknown Awareness operation: "${operation}".`, { status: 'unknown', operation }, true);
  if (query['describe'] === true) {
    const payload = { operation, use: descriptor.use, inputSchema: descriptor.inputSchema, effects: descriptor.effects };
    const bounded = boundedOutput(JSON.stringify(payload));
    return result(bounded.text, { status: bounded.truncated ? 'partial' : 'ok', operation, effect: 'read', ...bounded });
  }
  assertPersistentAwarenessEnabled();
  const params = record(query['params']) ?? {};
  const effect = descriptor.effect(params);
  const request = routineApproval(descriptor, params);
  if (request) {
    const approval = await requestApproval(ctx, request, signal);
    if (!approval.approved)
      return result(
        `Approval declined for Awareness ${operation}.`,
        { status: 'denied', operation, effect, approval },
        true,
      );
  }

  // The canonical History read defaults to 64 KiB. Base64 expansion makes that
  // exceed both the client and Pi model-output budgets, and the client's generic
  // one-row fallback would turn a byte stream into one-byte pages. Preflight an
  // omitted limit into a bounded byte continuation before any history read.
  if (operation === 'history.read' && params['limit'] === undefined) {
    const retry = {
      tool: 'awareness',
      queries: [{
        reasoning: 'Read a bounded Awareness History chunk',
        operation,
        params: { ...params, limit: NATIVE_HISTORY_READ_CHUNK_BYTES },
      }],
    };
    const payload = {
      partial: true,
      partialReasons: ['output_limit'],
      diagnostic: {
        kind: 'output-limit',
        code: 'AWARENESS_OUTPUT_LIMIT',
        limit: AWARENESS_OUTPUT_MAX_CHARS,
      },
      next: { retry },
      hint: 'History read defaults exceed the native output budget. Continue with the bounded byte limit.',
    };
    return result(JSON.stringify(payload), {
      status: 'partial',
      operation,
      effect,
      code: 2,
      output: payload,
      truncated: true,
    });
  }

  const bindings = buildAwarenessContext(ctx);
  const timeoutMs = typeof query['timeoutMs'] === 'number' ? query['timeoutMs'] : undefined;
  const sessionId = ctx?.sessionManager?.getSessionId?.();
  let execution;
  try {
    execution = await runner(
      { operation: descriptor.operation, ...(Object.keys(params).length ? { params } : {}) } as AwarenessExecutableCall,
      {
        ...bindings,
        signal,
        timeoutMs,
        ...(sessionId ? { sessionId } : {}),
      },
    );
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    return result(
      `Awareness ${operation} failed: ${message}`,
      { status: signal?.aborted ? 'cancelled' : 'failed', operation, effect, error: message },
      true,
    );
  }

  const parsedOutput = nativeOperationContinuations(execution.payload);
  const rawText = execution.text ?? JSON.stringify(parsedOutput);
  const diagnostics = execution.diagnostics?.join('\n');
  const readOnly = effect === 'read';
  const currentLimit = Number(params['limit'] ?? 0);
  let canRetry = readOnly && rawText.length > AWARENESS_OUTPUT_MAX_CHARS && params['limit'] !== 1;
  const narrower = {
    ...params,
    limit: Number.isSafeInteger(currentLimit) && currentLimit > 1
      ? Math.max(1, Math.floor(currentLimit / 2))
      : 1,
  };
  if (canRetry) {
    try {
      descriptor.validate(narrower);
    } catch {
      canRetry = false;
    }
  }
  const completedWrite = !readOnly && execution.exitCode === 0 && !execution.cancelled;
  const bounded = boundedOutput(
    rawText,
    canRetry
      ? {
          tool: 'awareness',
          queries: [{ reasoning: 'Read a smaller Awareness page', operation, params: narrower }],
        }
      : undefined,
    completedWrite,
    completedWrite && rawText.length > AWARENESS_OUTPUT_MAX_CHARS
      ? awarenessWriteReceipt(parsedOutput)
      : undefined,
  );
  const boundedStderr = diagnostics ? boundedOutput(diagnostics) : undefined;
  const validAttention = operation === 'work.verify'
    && params['action'] === 'audit'
    && execution.exitCode === 1
    && record(parsedOutput)?.['ok'] === true;
  const validPartial = readOnly
    && execution.exitCode === 2
    && record(execution.payload)?.['error_code'] === 'OUTPUT_BUDGET_EXCEEDED';
  const accepted = !execution.cancelled && (execution.exitCode === 0 || validAttention || validPartial);
  const status = execution.cancelled
    ? 'cancelled'
    : execution.exitCode === 0
      ? 'ok'
      : validPartial
        ? 'partial'
      : validAttention
        ? 'attention'
        : execution.exitCode === 2
          ? 'blocked'
          : 'failed';
  return result(
    bounded.text,
    {
      status,
      operation,
      effect,
      code: execution.exitCode,
      killed: Boolean(execution.cancelled),
      output: bounded.truncated && !completedWrite
        ? { truncated: true, totalChars: bounded.totalChars }
        : parsedOutput,
      truncated: bounded.truncated,
      totalChars: bounded.totalChars,
      ...(boundedStderr
        ? { stderr: boundedStderr.text, stderrTruncated: boundedStderr.truncated }
        : {}),
    },
    !accepted,
  );
}

export function registerAwarenessTool(
  pi: PiInstance,
  registeredToolNames: Set<string>,
  registerFn: RegisterFn,
  runner: AwarenessOperationRunner = runAwarenessOperation,
): void {
  const operationCount = ROUTINE_AWARENESS_OPERATIONS.length;
  const itemSchema = z.strictObject({
    operation: z.enum(ROUTINE_AWARENESS_OPERATIONS)
      .describe(`One of ${operationCount} canonical Awareness operations.`),
    describe: z.boolean().optional()
      .describe('Return the canonical operation schema without executing it. Do not include params.'),
    params: z.record(z.string(), z.unknown()).optional()
      .describe('Operation parameters. Pi binds database, workspace, session, and actor.'),
    timeoutMs: z.number().int().min(1).max(300_000).optional()
      .describe('Cooperative deadline in milliseconds; default 120000.'),
  });
  const parameters = buildQueryEnvelopeSchema(itemSchema, {
    maxItems: 100,
    reasoningDescription: 'Concise reason this Awareness operation is necessary.',
  });

  registerFn(pi, registeredToolNames, {
    name: 'awareness',
    label: 'awareness',
    description: DIRECT_TOOL_DESCRIPTIONS.awareness!,
    promptSnippet: `Call one of ${operationCount} bound Awareness operations directly. Start with context.orient.`,
    promptGuidelines: [
      'Use queries[] with reasoning, operation, and optional params. Example: {"queries":[{"reasoning":"Orient once","operation":"context.orient"}]}.',
      'Discover exact parameters without executing: {"queries":[{"reasoning":"Inspect operation schema","operation":"work.create","describe":true}]}. Do not include params with describe.',
      'Pi binds database, workspace, session, and actor. Follow executable next continuations; exit 2 is partial for output-budget recovery, otherwise blocked.',
    ],
    parameters,
    async execute(toolCallId, params, signal, onUpdate, ctx) {
      let stateChangingCalls = 0;
      const batchSize = Array.isArray(record(params)?.['queries'])
        ? (record(params)?.['queries'] as unknown[]).length
        : 0;
      return executeQueryBatch({
        raw: params,
        toolCallId,
        signal,
        onUpdate: onUpdate as ((value: ToolCallResult) => void) | undefined,
        ctx,
        passthroughSingle: true,
        summarize: (value, query) => {
          const status = String(record(value.details)?.['status'] ?? (value.isError ? 'failed' : 'ok'));
          return `${String(query['operation'] ?? '')} · ${status}`;
        },
        preflight: query => {
          validateOperationQuery(query);
          if (query['describe'] === true) return;
          assertPersistentAwarenessEnabled();
          const descriptor = getAwarenessOperationDescriptor(String(query['operation']));
          if (!descriptor) return;
          const effect = descriptor.effect(record(query['params']) ?? {});
          if (effect === 'read') return;
          stateChangingCalls += 1;
          if (batchSize > 1 || stateChangingCalls > 1) {
            throw new Error(
              'A batch may contain at most one state-changing Awareness operation because committed operations are not rolled back.',
            );
          }
        },
        execute: query => callOperation(runner, query, signal, ctx),
      });
    },
    renderCall(args: unknown, theme?: PiTheme) {
      const queries = record(args)?.['queries'];
      const first = Array.isArray(queries) ? record(queries[0]) : undefined;
      const operation = String(first?.['operation'] ?? '');
      return makeComponentRenderer(
        (_props, { width }) => [
          truncateToWidth(
            `${paint(theme, 'brand', '◆ awareness')} ${paint(theme, 'dim', '·')} ${paint(theme, 'title', operation)}`,
            width,
          ),
        ],
        undefined,
      );
    },
    renderResult(value: ToolCallResult, _opts, theme?: PiTheme) {
      const status = String(record(value.details)?.['status'] ?? (value.isError ? 'failed' : 'ok'));
      const icon = value.isError ? paint(theme, 'error', '✗') : paint(theme, 'success', '✓');
      return makeComponentRenderer(
        (_props, { width }) => [
          truncateToWidth(
            `${icon} ${paint(theme, 'title', 'awareness')} ${paint(theme, 'dim', `· ${status}`)}`,
            width,
          ),
        ],
        undefined,
      );
    },
  });
}
