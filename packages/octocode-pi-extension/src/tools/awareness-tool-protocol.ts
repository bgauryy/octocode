import {
  getAwarenessOperationDescriptor,
  type AwarenessOperationEffect,
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
  'session_id',
  'compact',
]);
export const AWARENESS_OUTPUT_MAX_CHARS = 12_000;

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
): AwarenessOperationEffect {
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
  const executableKeys = Object.keys(object);
  if (typeof object['operation'] === 'string'
    && executableKeys.every(key => key === 'operation' || key === 'params')
    && getAwarenessOperationDescriptor(object['operation'])) {
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
