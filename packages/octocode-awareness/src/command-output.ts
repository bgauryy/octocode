import { AsyncLocalStorage } from 'node:async_hooks';

/** Request-local output, shared by command handlers and their shell adapters. */
export interface AwarenessOperationOutput {
  command: string;
  compact: boolean;
  payload?: unknown;
  text: string;
  diagnostics: string[];
}
export const commandOutput = new AsyncLocalStorage<AwarenessOperationOutput>();

export class AwarenessInputError extends Error {
  constructor(message: string, readonly details: Record<string, unknown> = {}) { super(message); }
}

export function writeCommandText(text: string): void {
  const output = commandOutput.getStore();
  if (output) output.text += text;
  else process.stdout.write(text);
}

export function writeCommandDiagnostic(text: string): void {
  const output = commandOutput.getStore();
  if (output) output.diagnostics.push(text);
  else process.stderr.write(`${text}\n`);
}

export function writeCommandPayload(payload: unknown, compact = false): void {
  const output = commandOutput.getStore();
  if (output) output.payload = payload;
  else process.stdout.write(`${JSON.stringify(payload, null, compact ? 0 : 2)}\n`);
}

export interface EmitOptions { compact?: boolean; cli?: boolean }

function compactValue(value: unknown, key?: string): unknown {
  // Unknown identity labels are meaningful even in compact peer discovery.
  if (value === null && (key === 'agent_vendor' || key === 'agent_host')) return null;
  if (key === 'db_path' || value === null || value === undefined) return undefined;
  if (Array.isArray(value)) return value.map((item) => compactValue(item)).filter((item) => item !== undefined);
  if (typeof value !== 'object') return value;
  const out: Record<string, unknown> = {};
  for (const [childKey, childValue] of Object.entries(value as Record<string, unknown>)) {
    const compacted = compactValue(childValue, childKey);
    if (compacted !== undefined) out[childKey] = compacted;
  }
  if (key === 'filters') {
    delete out['limit'];
    for (const [filterKey, filterValue] of Object.entries(out)) {
      if (Array.isArray(filterValue) && filterValue.length === 0) delete out[filterKey];
    }
    if (Object.keys(out).length === 0) return undefined;
  }
  if (typeof out['count'] === 'number' && out['total'] === out['count']) delete out['total'];
  if (out['omitted_count'] === 0) delete out['omitted_count'];
  if (out['is_partial'] === false) delete out['is_partial'];
  if (typeof out['workspace_path'] === 'string' && Array.isArray(out['rows'])) {
    out['rows'] = out['rows'].map((row) => {
      if (!row || typeof row !== 'object' || Array.isArray(row)) return row;
      const projected = { ...(row as Record<string, unknown>) };
      if (projected['workspace_path'] === out['workspace_path']) delete projected['workspace_path'];
      return projected;
    });
  }
  return out;
}

export function emit(payload: Record<string, unknown>, exitCode = 0, opts: EmitOptions = {}): number {
  const context = commandOutput.getStore();
  payload['ok'] = payload['ok'] ?? (exitCode === 0);
  if (context && exitCode !== 0 && typeof payload['error'] === 'string' && payload['command'] === undefined) {
    payload['command'] = context.command;
  }
  const compact = context?.compact ?? opts.compact === true;
  const output = compact ? compactValue(payload) : payload;
  writeCommandPayload(output, compact);
  return exitCode;
}

export function die(message: string, extras: Record<string, unknown> = {}): never {
  throw new AwarenessInputError(message, extras);
}
