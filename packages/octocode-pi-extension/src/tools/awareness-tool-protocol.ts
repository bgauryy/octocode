import {
  getAwarenessCommandDescriptor,
  getAwarenessOperationDescriptor,
  listAwarenessCommandDescriptors,
  type AwarenessCommandDescriptor,
  type AwarenessCommandEffect,
  type AwarenessOperationDescriptor,
} from '@octocodeai/octocode-awareness';
import type { ApprovalClass } from '@octocodeai/agent-contracts/protocols';
import type { ToolCallResult } from '../types.js';

export const RESERVED_PARAMS = new Set([
  'db',
  'database',
  'workspace',
  'agent_id',
  'lead_agent_id',
  'compact',
]);
export const AWARENESS_OUTPUT_MAX_CHARS = 12_000;
const ROUTINE_LEGACY_COMMANDS = new Set(
  listAwarenessCommandDescriptors({ routine: true }).map(entry => entry.command)
);

export function result(
  text: string,
  details: Record<string, unknown>,
  isError = false
): ToolCallResult {
  return { content: [{ type: 'text', text }], details, isError };
}

export function record(value: unknown): Record<string, unknown> | undefined {
  return value && typeof value === 'object' && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : undefined;
}

export function routineEffect(
  descriptor: AwarenessOperationDescriptor,
  params: Record<string, unknown>
): AwarenessCommandEffect {
  if (descriptor.operation === 'history.restore')
    return params['action'] === 'apply' ? 'workspace-write' : 'read';
  if (descriptor.operation === 'work.protect')
    return params['action'] === 'wait' ? 'read' : 'coordination-write';
  if (descriptor.operation === 'work.verify')
    return params['action'] === 'audit' ? 'read' : 'coordination-write';
  return descriptor.effects.length === 1
    ? descriptor.effects[0]!
    : 'coordination-write';
}

export function validateRoutineParams(
  descriptor: AwarenessOperationDescriptor,
  params: Record<string, unknown>
): void {
  const reservedOverride = Object.keys(params).find(key => RESERVED_PARAMS.has(key));
  if (reservedOverride)
    throw new Error(`${reservedOverride} is host-injected and cannot be overridden`);
  const selections: Partial<Record<AwarenessOperationDescriptor['operation'], [string, readonly string[]]>> = {
    'work.create': ['kind', ['plan', 'task', 'standalone']],
    'work.show': ['kind', ['plan', 'task', 'presence']],
    'work.update': ['transition', ['heartbeat', 'submit', 'release', 'retry', 'touch', 'end', 'join', 'document', 'status']],
    'work.protect': ['action', ['acquire', 'wait', 'release']],
    'work.verify': ['action', ['audit', 'mark']],
  };
  const selection = selections[descriptor.operation];
  if (selection && !selection[1].includes(String(params[selection[0]] ?? '')))
    throw new Error(`${descriptor.operation} ${selection[0]} must be one of: ${selection[1].join(', ')}`);
  if (descriptor.operation === 'history.restore' && params['action'] !== undefined
    && !['preview', 'apply'].includes(String(params['action'])))
    throw new Error('history.restore action must be preview or apply');
}

export function routineApproval(
  descriptor: AwarenessOperationDescriptor,
  params: Record<string, unknown>
): { actionClass: ApprovalClass; title: string; detail: string } | undefined {
  if (routineEffect(descriptor, params) !== 'workspace-write') return undefined;
  const parameterText = JSON.stringify(params);
  return {
    actionClass: 'fs-delete',
    title: `Allow Awareness ${descriptor.operation}?`,
    detail: `Apply a previewed workspace restore. Parameters: ${parameterText.slice(0, 1_000)}${parameterText.length > 1_000 ? '…' : ''}`,
  };
}

export function nativeOperationContinuations(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(nativeOperationContinuations);
  const object = record(value);
  if (!object) return value;
  if (typeof object['operation'] === 'string' && getAwarenessOperationDescriptor(object['operation'])) {
    const params = record(object['params']);
    return {
      tool: 'awareness',
      queries: [{
        reasoning: 'Continue the requested Awareness results',
        operation: object['operation'],
        ...(params ? { params } : {}),
      }],
    };
  }
  return Object.fromEntries(Object.entries(object).map(([key, child]) => [
    key,
    ['next', 'actions', 'continuations'].includes(key)
      ? nativeOperationContinuations(child)
      : child,
  ]));
}

export function markLegacyContinuationEnvelopes(value: unknown, withinContinuation = false): unknown {
  if (Array.isArray(value))
    return value.map(child => markLegacyContinuationEnvelopes(child, withinContinuation));
  const object = record(value);
  if (!object) return value;
  if (object['tool'] === 'awareness' && Array.isArray(object['queries'])) {
    return {
      ...object,
      queries: object['queries'].map(query => {
        const item = record(query);
        return item?.['action'] && item['operation'] === undefined
          ? { ...item, operation: 'legacy' }
          : query;
      }),
    };
  }
  return Object.fromEntries(Object.entries(object).map(([key, child]) => {
    const descend = withinContinuation || ['next', 'actions', 'continuations'].includes(key);
    return [key, descend ? markLegacyContinuationEnvelopes(child, true) : child];
  }));
}

export function nativeInputSchema(
  descriptor: AwarenessCommandDescriptor
): Record<string, unknown> {
  const schema = structuredClone(descriptor.inputSchema) as Record<string, unknown>;
  const removeBindings = (variant: Record<string, unknown>): void => {
    const properties = record(variant['properties']);
    if (properties)
      for (const field of RESERVED_PARAMS) delete properties[field];
    if (Array.isArray(variant['required'])) {
      const required = variant['required'].filter(
        field => typeof field === 'string' && !RESERVED_PARAMS.has(field)
      );
      if (required.length) variant['required'] = required;
      else delete variant['required'];
    }
    for (const kind of ['oneOf', 'anyOf', 'allOf']) {
      const branches = variant[kind];
      if (Array.isArray(branches))
        for (const branch of branches) {
          const child = record(branch);
          if (child) removeBindings(child);
        }
    }
  };
  removeBindings(schema);
  schema['x-pi-host-injected'] = descriptor.injected;
  return schema;
}

function modelInputSchema(descriptor: AwarenessCommandDescriptor): Record<string, unknown> {
  const schema = nativeInputSchema(descriptor);
  delete schema['$schema'];
  delete schema['description'];
  for (const key of Object.keys(schema)) {
    if (key.startsWith('x-')) delete schema[key];
  }
  return schema;
}

function modelDescriptor(descriptor: AwarenessCommandDescriptor): Record<string, unknown> {
  return {
    command: descriptor.command,
    use: descriptor.use,
    effect: descriptor.effect,
    piMode: descriptor.piMode,
    ...(descriptor.approvalClass ? { approvalClass: descriptor.approvalClass } : {}),
    hostInjected: descriptor.injected,
    inputSchema: modelInputSchema(descriptor),
  };
}

export function boundedOutput(
  raw: string,
  retry?: Record<string, unknown>,
  completedWrite = false,
  receipt?: Record<string, unknown>
): { text: string; totalChars: number; truncated: boolean } {
  if (raw.length <= AWARENESS_OUTPUT_MAX_CHARS)
    return { text: raw, totalChars: raw.length, truncated: false };
  return {
    text: JSON.stringify({
      partial: true,
      partialReasons: ['output_limit'],
      diagnostic: {
        kind: retry ? 'output-limit' : 'terminal-limit',
        code: 'AWARENESS_OUTPUT_LIMIT',
        totalChars: raw.length,
        limit: AWARENESS_OUTPUT_MAX_CHARS,
      },
      ...(retry ? { next: { retry } } : {}),
      ...(completedWrite ? { commandCompleted: true } : {}),
      ...(receipt ? { receipt } : {}),
      hint: completedWrite
        ? 'The command completed, but its response exceeds the native output limit. Do not repeat the write to recover output; inspect the resulting state with a read command.'
        : 'The response exceeds the native output limit. Describe this command and narrow its limit, filters or detail options before retrying. This is not a complete result page.',
    }),
    totalChars: raw.length,
    truncated: true,
  };
}

export function approvalRequest(
  descriptor: AwarenessCommandDescriptor,
  params: Record<string, unknown>
): { actionClass: ApprovalClass; title: string; detail: string } | undefined {
  if (!descriptor.approvalClass) return undefined;
  const parameterText = JSON.stringify(params);
  return {
    actionClass: descriptor.approvalClass,
    title: `Allow Awareness ${descriptor.command}?`,
    detail: `${descriptor.effect} command in ${descriptor.piMode} mode. Parameters: ${parameterText.slice(0, 1_000)}${parameterText.length > 1_000 ? '…' : ''}`,
  };
}

export const STIER_HINT =
  'Use a canonical operation directly. For operator or recovery commands, use operation:"legacy" with action:"list" and a narrow noun filter.';

export function listCommands(query: Record<string, unknown>): ToolCallResult {
  const noun = typeof query['command'] === 'string'
    ? query['command'].trim()
    : typeof query['noun'] === 'string'
      ? query['noun'].trim()
      : '';
  const all = query['all'] === true;
  const page = typeof query['page'] === 'number' ? query['page'] : 1;
  const pageSize = typeof query['pageSize'] === 'number' ? query['pageSize'] : 10;
  const filtered = listAwarenessCommandDescriptors().filter(
    entry => (all || !ROUTINE_LEGACY_COMMANDS.has(entry.command))
      && (!noun || entry.command === noun || entry.command.startsWith(`${noun} `))
  );
  const start = (page - 1) * pageSize;
  const entries = filtered.slice(start, start + pageSize).map(entry => ({
    command: entry.command,
    use: entry.use,
    effect: entry.effect,
    injected: entry.injected,
    approvalClass: entry.approvalClass,
    piMode: entry.piMode,
  }));
  const hasMore = start + entries.length < filtered.length;
  const next = hasMore
    ? {
        tool: 'awareness',
        queries: [{
          reasoning: 'Continue listing legacy Awareness commands',
          operation: 'legacy',
          action: 'list',
          ...(noun ? { command: noun } : {}),
          ...(all ? { all: true } : {}),
          page: page + 1,
        }],
      }
    : undefined;
  const payload = { count: filtered.length, page, pageSize, entries, ...(next ? { next } : {}) };
  const modelPayload = {
    count: filtered.length,
    entries: entries.map(entry => ({
      command: entry.command,
      use: entry.use,
      effect: entry.effect,
      ...(entry.piMode !== 'normal' ? { piMode: entry.piMode } : {}),
      ...(entry.approvalClass ? { approvalClass: entry.approvalClass } : {}),
    })),
    ...(next ? { next } : {}),
  };
  return result(JSON.stringify(modelPayload), { status: 'listed', ...payload });
}

export function describeCommand(query: Record<string, unknown>): ToolCallResult {
  const command = String(query['command'] ?? '').trim();
  const descriptor = getAwarenessCommandDescriptor(command);
  if (!descriptor)
    return result(`Unknown Awareness command: "${command}". ${STIER_HINT}`, { status: 'unknown', command }, true);
  const payload = { ...descriptor, inputSchema: nativeInputSchema(descriptor) };
  return result(JSON.stringify(modelDescriptor(descriptor)), {
    status: 'described', command, descriptor: payload,
  });
}
