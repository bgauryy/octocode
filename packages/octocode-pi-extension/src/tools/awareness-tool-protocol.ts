import {
  getAwarenessOperationDescriptor,
  type AwarenessOperationDescriptor,
} from '@octocodeai/octocode-awareness';
import { AWARENESS_HOST_PARAMETER_NAMES } from '@octocodeai/octocode-awareness/host';
import type { ApprovalClass } from '@octocodeai/agent-contracts/protocols';
import type { ToolCallResult } from '../types.js';

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

export function validateRoutineParams(
  descriptor: AwarenessOperationDescriptor,
  params: Record<string, unknown>
): void {
  const reservedOverride = AWARENESS_HOST_PARAMETER_NAMES.find(key => params[key] !== undefined);
  if (reservedOverride)
    throw new Error(`${reservedOverride} is host-injected and cannot be overridden`);
  descriptor.validate(params);
}

export function routineApproval(
  descriptor: AwarenessOperationDescriptor,
  params: Record<string, unknown>
): { actionClass: ApprovalClass; title: string; detail: string } | undefined {
  const actionClass = descriptor.approval(params);
  if (!actionClass) return undefined;
  const parameterText = JSON.stringify(params);
  return {
    actionClass,
    title: `Allow Awareness ${descriptor.operation}?`,
    detail: `Execute approved ${descriptor.operation}. Parameters: ${parameterText.slice(0, 1_000)}${parameterText.length > 1_000 ? '…' : ''}`,
  };
}

export function nativeOperationContinuations(value: unknown, withinContinuation = false): unknown {
  if (Array.isArray(value)) return value.map(child => nativeOperationContinuations(child, withinContinuation));
  const object = record(value);
  if (!object) return value;
  const executableKeys = Object.keys(object);
  if (withinContinuation && typeof object['operation'] === 'string'
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
    withinContinuation || ['next', 'actions', 'continuations', 'undo_preview'].includes(key)
      ? nativeOperationContinuations(child, true)
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
