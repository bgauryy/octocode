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
import { compileMcpSchemaValidator } from './mcp/schema-validator.js';
import {
  AWARENESS_OUTPUT_MAX_CHARS,
  boundedOutput,
  nativeOperationContinuations,
  record,
  result,
  routineApproval,
  routineEffect,
  validateRoutineParams,
} from './awareness-tool-protocol.js';

type RegisterFn = typeof registerUniqueTool;

function validateOperationQuery(query: Record<string, unknown>): void {
  const operation = String(query['operation'] ?? '').trim();
  const descriptor = getAwarenessOperationDescriptor(operation);
  if (!descriptor) throw new Error(`Unknown Awareness operation: "${operation}"`);
  const obsoleteField = ['action', 'command', 'noun', 'all', 'page', 'pageSize']
    .find(field => query[field] !== undefined);
  if (obsoleteField) throw new Error(`${obsoleteField} is not part of the canonical Awareness surface`);
  const params = record(query['params']) ?? {};
  validateRoutineParams(descriptor, params);
  const validation = compileMcpSchemaValidator(descriptor.inputSchema).validate(params);
  if (!validation.valid) {
    throw new Error(
      `Invalid parameters for Awareness ${operation}: ${validation.errors
        .map(error => `${error.instancePath || '/'} ${error.message}`).join('; ')}`,
    );
  }
}

async function callOperation(
  runner: AwarenessOperationRunner,
  query: Record<string, unknown>,
  signal?: AbortSignal,
  ctx?: PiContext,
): Promise<ToolCallResult> {
  assertPersistentAwarenessEnabled();
  const operation = String(query['operation'] ?? '').trim();
  const descriptor = getAwarenessOperationDescriptor(operation);
  if (!descriptor)
    return result(`Unknown Awareness operation: "${operation}".`, { status: 'unknown', operation }, true);
  const params = record(query['params']) ?? {};
  const effect = routineEffect(descriptor, params);
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
  const canRetry = readOnly && rawText.length > AWARENESS_OUTPUT_MAX_CHARS && params['limit'] !== 1;
  const narrower = {
    ...params,
    limit: Number.isSafeInteger(currentLimit) && currentLimit > 1
      ? Math.max(1, Math.floor(currentLimit / 2))
      : 1,
  };
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
  const accepted = !execution.cancelled && (execution.exitCode === 0 || validAttention);
  const status = execution.cancelled
    ? 'cancelled'
    : execution.exitCode === 0
      ? 'ok'
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
  const itemSchema = z.strictObject({
    operation: z.enum(ROUTINE_AWARENESS_OPERATIONS)
      .describe('One of 19 canonical Awareness operations.'),
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
    promptSnippet: 'Call one of 19 bound Awareness operations directly. Start with context.orient.',
    promptGuidelines: [
      'Use queries[] with reasoning, operation, and optional params. Example: {"queries":[{"reasoning":"Orient once","operation":"context.orient"}]}.',
      'Pi binds database, workspace, session, and actor. Follow executable next continuations; exit 2 means blocked.',
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
          assertPersistentAwarenessEnabled();
          const descriptor = getAwarenessOperationDescriptor(String(query['operation']));
          if (!descriptor) return;
          const effect = routineEffect(descriptor, record(query['params']) ?? {});
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
