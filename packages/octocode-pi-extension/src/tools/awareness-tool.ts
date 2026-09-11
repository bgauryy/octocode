import {
  ROUTINE_AWARENESS_OPERATIONS,
  getAwarenessCommandDescriptor,
  getAwarenessOperationDescriptor,
  type AwarenessCommandEffect,
  type AwarenessExecutableCall,
} from '@octocodeai/octocode-awareness';
import { z } from 'zod';
import type { PiContext, PiInstance, PiTheme, ToolCallResult } from '../types.js';
import { DIRECT_TOOL_DESCRIPTIONS, type registerUniqueTool } from './octocode-tools.js';
import { buildQueryEnvelopeSchema, executeQueryBatch } from './query-envelope.js';
import { nativeContinuations } from './awareness-continuations.js';
import { awarenessWriteReceipt } from './awareness-output.js';
import { buildAwarenessContext } from './awareness-context.js';
import { assertPersistentAwarenessEnabled } from './storage-policy.js';
import { requestApproval } from './approval.js';
import {
  runAwarenessCommand,
  type AwarenessCommandRunner,
} from './awareness-command-runner.js';
import { makeComponentRenderer } from './render-helpers.js';
import { compileMcpSchemaValidator } from './mcp/schema-validator.js';
import { truncateToWidth } from '../tui/width.js';
import { paint } from '../tui/palette.js';
import {
  AWARENESS_OUTPUT_MAX_CHARS,
  RESERVED_PARAMS,
  STIER_HINT,
  approvalRequest,
  boundedOutput,
  describeCommand,
  listCommands,
  markLegacyContinuationEnvelopes,
  nativeInputSchema,
  nativeOperationContinuations,
  record,
  result,
  routineApproval,
  routineEffect,
  validateRoutineParams,
} from './awareness-tool-protocol.js';

type RegisterFn = typeof registerUniqueTool;

async function callCommand(
  runner: AwarenessCommandRunner,
  query: Record<string, unknown>,
  signal?: AbortSignal,
  ctx?: PiContext
): Promise<ToolCallResult> {
  assertPersistentAwarenessEnabled();
  const command = String(query['command'] ?? '').trim();
  const descriptor = getAwarenessCommandDescriptor(command);
  if (!descriptor)
    return result(
      `Unknown Awareness command: "${command}". ${STIER_HINT}`,
      { status: 'unknown', command },
      true
    );
  if (descriptor.piMode === 'external-host-only') {
    return result(
      `${command} is external-host-only and cannot run through Pi's native Awareness facade.`,
      { status: 'unsupported', command, piMode: descriptor.piMode },
      true
    );
  }

  const params = record(query['params']) ?? {};
  const reservedOverride = Object.keys(params).find(key =>
    RESERVED_PARAMS.has(key)
  );
  if (reservedOverride) {
    return result(
      `${reservedOverride} is host-injected and cannot be overridden`,
      {
        status: 'invalid',
        command,
        effect: descriptor.effect,
        field: reservedOverride,
      },
      true
    );
  }
  const validation = compileMcpSchemaValidator(
    nativeInputSchema(descriptor)
  ).validate(params);
  if (!validation.valid) {
    return result(
      `Invalid parameters for Awareness ${command}: ${validation.errors.map(error => `${error.instancePath || '/'} ${error.message}`).join('; ')}`,
      {
        status: 'invalid',
        command,
        effect: descriptor.effect,
        errors: validation.errors,
      },
      true
    );
  }

  const request = approvalRequest(descriptor, params);
  if (request) {
    const approval = await requestApproval(ctx, request, signal);
    if (!approval.approved) {
      return result(
        `Approval declined for Awareness ${command}.`,
        { status: 'denied', command, effect: descriptor.effect, approval },
        true
      );
    }
  }

  const bindings = buildAwarenessContext(ctx);
  const timeoutMs =
    typeof query['timeoutMs'] === 'number' ? query['timeoutMs'] : undefined;
  let execution;
  try {
    execution = await runner(
      { command, params },
      { ...bindings, signal, timeoutMs }
    );
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    return result(
      `Awareness ${command} failed: ${message}`,
      {
        status: signal?.aborted ? 'cancelled' : 'failed',
        command,
        effect: descriptor.effect,
        error: message,
      },
      true
    );
  }

  const parsedOutput = markLegacyContinuationEnvelopes(
    nativeContinuations(execution.payload, RESERVED_PARAMS, command)
  );
  const rawText = execution.text ?? JSON.stringify(parsedOutput);
  const diagnostics = execution.diagnostics?.join('\n');
  // History limits count bytes, not rows. A one-byte retry preserves correctness
  // but can turn one read into tens of thousands of model/tool round trips.
  const limitSchema = record(record(descriptor.inputSchema['properties'])?.['limit']);
  const historyContent = command === 'history read' ? record(parsedOutput)?.['content'] : undefined;
  const currentLimit = typeof historyContent === 'string'
    ? Buffer.byteLength(historyContent, 'base64')
    : Number(params['limit'] ?? limitSchema?.['default']);
  const retryLimit = command === 'history read' && Number.isSafeInteger(currentLimit) && currentLimit > 1
    ? Math.max(1, Math.min(currentLimit - 1, Math.floor(currentLimit * AWARENESS_OUTPUT_MAX_CHARS * 0.75 / rawText.length)))
    : 1;
  const narrower = { ...params, limit: retryLimit };
  const canRetry =
    descriptor.effect === 'read' &&
    rawText.length > AWARENESS_OUTPUT_MAX_CHARS &&
    params['limit'] !== 1 &&
    Object.hasOwn(
      record(descriptor.inputSchema['properties']) ?? {},
      'limit'
    ) &&
    compileMcpSchemaValidator(nativeInputSchema(descriptor)).validate(narrower)
      .valid;
  const completedWrite = descriptor.effect !== 'read' && execution.exitCode === 0 && !execution.cancelled;
  const bounded = boundedOutput(
    rawText,
    canRetry
      ? {
          tool: 'awareness',
          queries: [
            {
              reasoning: 'Read a smaller Awareness page',
              operation: 'legacy',
              action: 'call',
              command,
              params: narrower,
            },
          ],
        }
      : undefined,
    completedWrite,
    completedWrite && rawText.length > AWARENESS_OUTPUT_MAX_CHARS ? awarenessWriteReceipt(parsedOutput) : undefined
  );
  const boundedStderr = diagnostics ? boundedOutput(diagnostics) : undefined;
  const validReportExit =
    execution.exitCode !== null &&
    descriptor.resultExitCodes?.includes(execution.exitCode) === true &&
    record(parsedOutput)?.['ok'] === true;
  const accepted =
    !execution.cancelled && (execution.exitCode === 0 || validReportExit);
  const status = execution.cancelled
    ? 'cancelled'
    : execution.exitCode === 0
      ? 'ok'
      : validReportExit
        ? 'attention'
        : execution.exitCode === 2
          ? 'blocked'
          : 'failed';
  return result(
    bounded.text,
    {
      status,
      command,
      effect: descriptor.effect,
      piMode: descriptor.piMode,
      code: execution.exitCode,
      killed: Boolean(execution.cancelled),
      output: bounded.truncated && !completedWrite
        ? { truncated: true, totalChars: bounded.totalChars }
        : parsedOutput,
      truncated: bounded.truncated,
      totalChars: bounded.totalChars,
      ...(boundedStderr
        ? {
            stderr: boundedStderr.text,
            stderrTruncated: boundedStderr.truncated,
          }
        : {}),
    },
    !accepted
  );
}

async function callOperation(
  runner: AwarenessCommandRunner,
  query: Record<string, unknown>,
  signal?: AbortSignal,
  ctx?: PiContext
): Promise<ToolCallResult> {
  assertPersistentAwarenessEnabled();
  const operation = String(query['operation'] ?? '').trim();
  const descriptor = getAwarenessOperationDescriptor(operation);
  if (!descriptor)
    return result(`Unknown Awareness operation: "${operation}".`, { status: 'unknown', operation }, true);
  const params = record(query['params']) ?? {};
  try {
    validateRoutineParams(descriptor, params);
  } catch (error) {
    return result(error instanceof Error ? error.message : String(error), {
      status: 'invalid', operation,
    }, true);
  }
  const effect = routineEffect(descriptor, params);
  const request = routineApproval(descriptor, params);
  if (request) {
    const approval = await requestApproval(ctx, request, signal);
    if (!approval.approved)
      return result(`Approval declined for Awareness ${operation}.`, { status: 'denied', operation, effect, approval }, true);
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
      }
    );
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    return result(`Awareness ${operation} failed: ${message}`, {
      status: signal?.aborted ? 'cancelled' : 'failed', operation, effect, error: message,
    }, true);
  }

  const parsedOutput = nativeOperationContinuations(execution.payload);
  const rawText = execution.text ?? JSON.stringify(parsedOutput);
  const diagnostics = execution.diagnostics?.join('\n');
  const readOnly = effect === 'read';
  const currentLimit = Number(params['limit'] ?? 0);
  const canRetry = readOnly && rawText.length > AWARENESS_OUTPUT_MAX_CHARS && params['limit'] !== 1;
  const narrower = { ...params, limit: Number.isSafeInteger(currentLimit) && currentLimit > 1 ? Math.max(1, Math.floor(currentLimit / 2)) : 1 };
  const completedWrite = !readOnly && execution.exitCode === 0 && !execution.cancelled;
  const bounded = boundedOutput(
    rawText,
    canRetry ? {
      tool: 'awareness',
      queries: [{ reasoning: 'Read a smaller Awareness page', operation, params: narrower }],
    } : undefined,
    completedWrite,
    completedWrite && rawText.length > AWARENESS_OUTPUT_MAX_CHARS ? awarenessWriteReceipt(parsedOutput) : undefined
  );
  const boundedStderr = diagnostics ? boundedOutput(diagnostics) : undefined;
  const validAttention = operation === 'work.verify' && params['action'] === 'audit'
    && execution.exitCode === 1 && record(parsedOutput)?.['ok'] === true;
  const accepted = !execution.cancelled && (execution.exitCode === 0 || validAttention);
  const status = execution.cancelled ? 'cancelled'
    : execution.exitCode === 0 ? 'ok'
      : validAttention ? 'attention'
        : execution.exitCode === 2 ? 'blocked' : 'failed';
  return result(bounded.text, {
    status,
    operation,
    effect,
    code: execution.exitCode,
    killed: Boolean(execution.cancelled),
    output: bounded.truncated && !completedWrite ? { truncated: true, totalChars: bounded.totalChars } : parsedOutput,
    truncated: bounded.truncated,
    totalChars: bounded.totalChars,
    ...(boundedStderr ? { stderr: boundedStderr.text, stderrTruncated: boundedStderr.truncated } : {}),
  }, !accepted);
}

function preflightAwarenessQuery(query: Record<string, unknown>): void {
  // Old programmatic callers can still reach the bounded legacy lane. The
  // model-facing schema always requires an explicit operation.
  const operation = String(query['operation'] ?? (query['action'] ? 'legacy' : '')).trim();
  if (operation !== 'legacy') {
    const descriptor = getAwarenessOperationDescriptor(operation);
    if (!descriptor) throw new Error(`Unknown Awareness operation: "${operation}"`);
    const legacyField = ['action', 'command', 'all', 'page', 'pageSize'].find(field => query[field] !== undefined);
    if (legacyField) throw new Error(`${legacyField} is only valid with operation:"legacy"`);
    assertPersistentAwarenessEnabled();
    validateRoutineParams(descriptor, record(query['params']) ?? {});
    return;
  }
  const action = String(query['action'] ?? '');
  if (!action) throw new Error('operation:"legacy" requires action');
  if (action === 'list') return;
  const command = String(query['command'] ?? '').trim();
  if (!command) throw new Error(`${action} requires a non-empty command`);
  const descriptor = getAwarenessCommandDescriptor(command);
  if (!descriptor)
    throw new Error(`Unknown Awareness command: "${command}". ${STIER_HINT}`);
  if (action === 'describe') return;
  if (action !== 'call') throw new Error(`Unknown legacy Awareness action: ${action}`);
  assertPersistentAwarenessEnabled();
  if (descriptor.piMode === 'external-host-only') {
    throw new Error(
      `${command} is external-host-only; this callback is invoked by the host lifecycle`
    );
  }
  const params = record(query['params']) ?? {};
  const reservedOverride = Object.keys(params).find(key =>
    RESERVED_PARAMS.has(key)
  );
  if (reservedOverride)
    throw new Error(
      `${reservedOverride} is host-injected and cannot be overridden`
    );
  const validation = compileMcpSchemaValidator(
    nativeInputSchema(descriptor)
  ).validate(params);
  if (!validation.valid) {
    throw new Error(
      `Invalid parameters for Awareness ${command}: ${validation.errors.map(error => `${error.instancePath || '/'} ${error.message}`).join('; ')}`
    );
  }
}

export function registerAwarenessTool(
  pi: PiInstance,
  registeredToolNames: Set<string>,
  registerFn: RegisterFn,
  runner: AwarenessCommandRunner = runAwarenessCommand
): void {
  const itemSchema = z.object({
    operation: z
      .enum([...ROUTINE_AWARENESS_OPERATIONS, 'legacy'])
      .describe('Direct routine operation. Use legacy only for explicit operator or recovery commands.'),
    action: z.enum(['list', 'describe', 'call']).optional()
      .describe('Legacy-only action; routine operations execute directly.'),
    command: z
      .string()
      .optional()
      .describe(
        'Legacy command, or top-level noun filter for legacy list. Required for legacy describe/call.'
      ),
    params: z
      .record(z.string(), z.unknown())
      .optional()
      .describe(
        'Operation parameters. Pi injects database, workspace, and actor; never include those fields.'
      ),
    all: z.boolean().optional().describe('Legacy list only: include routine backing commands too.'),
    page: z.number().int().min(1).optional().describe('list page, 1-based.'),
    timeoutMs: z
      .number()
      .int()
      .min(1)
      .max(300_000)
      .optional()
      .describe(
        'Cooperative call deadline in milliseconds; default 120000. Atomic operations finish before cancellation is reported.'
      ),
  });
  const parameters = buildQueryEnvelopeSchema(itemSchema, {
    maxItems: 100,
    reasoningDescription:
      'Concise reason this Awareness operation is necessary.',
  });

  registerFn(pi, registeredToolNames, {
    name: 'awareness',
    label: 'awareness',
    description: DIRECT_TOOL_DESCRIPTIONS.awareness!,
    promptSnippet:
      'Call one of 19 bound Awareness operations directly. Start with context.orient.',
    promptGuidelines: [
      'Use queries[] with reasoning, operation, and optional params. Example: {"queries":[{"reasoning":"Orient once","operation":"context.orient"}]}.',
      'Pi binds database, workspace, session, and actor. Follow executable next continuations; exit 2 means blocked.',
      'Use operation:"legacy" with action:"list" or "describe" only for an explicit operator/recovery command.',
    ],
    parameters,
    async execute(toolCallId, params, signal, onUpdate, ctx) {
      let stateChangingCalls = 0;
      let routineCalls = 0;
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
          const resultDetails = record(value.details);
          const operation = String(query['operation'] ?? 'legacy');
          const target = operation === 'legacy'
            ? `${String(query['action'] ?? '')} ${String(query['command'] ?? '')}`.trim()
            : operation;
          return `${target} · ${String(resultDetails?.['status'] ?? (value.isError ? 'failed' : 'ok'))}`;
        },
        preflight: query => {
          preflightAwarenessQuery(query);
          const operation = String(query['operation'] ?? 'legacy');
          let effect: AwarenessCommandEffect = 'read';
          if (operation === 'legacy') {
            if (query['action'] !== 'call') return;
            const descriptor = getAwarenessCommandDescriptor(String(query['command'] ?? '').trim());
            if (!descriptor) return;
            effect = descriptor.effect;
          } else {
            routineCalls += 1;
            const descriptor = getAwarenessOperationDescriptor(operation);
            if (!descriptor) return;
            effect = routineEffect(descriptor, record(query['params']) ?? {});
            if (effect !== 'read' && batchSize > 1) {
              throw new Error('A batch may contain at most one state-changing Awareness operation; routine mutations must be the only query because committed operations are not rolled back.');
            }
            if (effect === 'read' && stateChangingCalls > 0) {
              throw new Error('Routine reads cannot share a batch with a state-changing legacy command.');
            }
          }
          if (effect === 'read') return;
          stateChangingCalls += 1;
          if (operation === 'legacy' && routineCalls > 0) {
            throw new Error('Legacy mutations cannot share a batch with routine operations.');
          }
          if (stateChangingCalls > 1) {
            throw new Error(
              operation === 'legacy'
                ? 'A batch may contain at most one state-changing Awareness command because committed commands are not rolled back. Split mutations into separate tool calls.'
                : 'A batch may contain at most one state-changing Awareness operation because committed operations are not rolled back. Split mutations into separate tool calls.'
            );
          }
        },
        execute: async query => {
          const operation = String(query['operation'] ?? 'legacy');
          if (operation !== 'legacy') return callOperation(runner, query, signal, ctx);
          const action = String(query['action'] ?? '');
          if (action === 'list') return listCommands(query);
          if (action === 'describe') return describeCommand(query);
          if (action === 'call') return callCommand(runner, query, signal, ctx);
          return result(
            `Unknown Awareness action: ${action}`,
            { status: 'unknown', action },
            true
          );
        },
      });
    },
    renderCall(args: unknown, theme?: PiTheme) {
      const queries = record(args)?.['queries'];
      const first = Array.isArray(queries) ? record(queries[0]) : undefined;
      const operation = String(first?.['operation'] ?? 'legacy');
      const target = operation === 'legacy'
        ? `${String(first?.['action'] ?? '')} ${String(first?.['command'] ?? '')}`.trim()
        : operation;
      return makeComponentRenderer(
        (_props, { width }) => [
          truncateToWidth(
            `${paint(theme, 'brand', '◆ awareness')} ${paint(theme, 'dim', '·')} ${paint(theme, 'title', target)}`,
            width
          ),
        ],
        undefined
      );
    },
    renderResult(value: ToolCallResult, _opts, theme?: PiTheme) {
      const status = String(
        record(value.details)?.['status'] ?? (value.isError ? 'failed' : 'ok')
      );
      const icon = value.isError
        ? paint(theme, 'error', '✗')
        : paint(theme, 'success', '✓');
      return makeComponentRenderer(
        (_props, { width }) => [
          truncateToWidth(
            `${icon} ${paint(theme, 'title', 'awareness')} ${paint(theme, 'dim', `· ${status}`)}`,
            width
          ),
        ],
        undefined
      );
    },
  });
}
