import { isRecord } from '../shared/util.js';

/**
 * The wire protocol of the external API: JSON-RPC 2.0 (the framing MCP, ACP and Codex's app-server also use).
 * Over the Unix socket every message is one JSON line; over HTTP a request is one POST and events arrive as SSE.
 */
export const PROTOCOL_VERSION = 1;

export const ERR = {
  parse: -32700,
  invalidRequest: -32600,
  methodNotFound: -32601,
  invalidParams: -32602,
  internal: -32603,
  /** The instance is not ready (no session) or is shutting down. */
  unavailable: -32000,
  /** A `wait` ran out of time; the message itself was accepted. */
  timeout: -32001,
} as const;

type RpcId = string | number | null;
export interface RpcRequest {
  jsonrpc: '2.0';
  id?: RpcId;
  method: string;
  params?: Record<string, unknown>;
}
export type RpcResponse = { jsonrpc: '2.0'; id: RpcId; result: unknown } | { jsonrpc: '2.0'; id: RpcId; error: { code: number; message: string; data?: unknown } };

/** A failure with a JSON-RPC code, thrown by method handlers. */
export class RpcError extends Error {
  constructor(
    readonly code: number,
    message: string,
    readonly data?: unknown,
  ) {
    super(message);
  }
}

export const MAX_FRAME_BYTES = 1_048_576;

/** Parse one frame into a request; returns an error response when it is not a valid JSON-RPC request. */
export function parseRequest(text: string): RpcRequest | RpcResponse {
  let value: unknown;
  try {
    value = JSON.parse(text);
  } catch {
    return failure(null, ERR.parse, 'Parse error');
  }
  if (!isRecord(value) || value['jsonrpc'] !== '2.0' || typeof value['method'] !== 'string') return failure(null, ERR.invalidRequest, 'Invalid request');
  const id = value['id'];
  if (id !== undefined && id !== null && typeof id !== 'string' && typeof id !== 'number') return failure(null, ERR.invalidRequest, 'Invalid request id');
  const params = value['params'];
  if (params !== undefined && !isRecord(params)) return failure(id ?? null, ERR.invalidParams, 'params must be an object');
  return { jsonrpc: '2.0', ...(id !== undefined ? { id } : {}), method: value['method'], ...(params ? { params } : {}) };
}

export const isResponse = (value: RpcRequest | RpcResponse): value is RpcResponse => !('method' in value);

export function failure(id: RpcId, code: number, message: string, data?: unknown): RpcResponse {
  return { jsonrpc: '2.0', id, error: { code, message, ...(data !== undefined ? { data } : {}) } };
}

export const success = (id: RpcId, result: unknown): RpcResponse => ({ jsonrpc: '2.0', id, result });

/** An event the instance publishes. `seq` is per instance and strictly increasing, so a client can resume with `since`. */
export interface ApiEvent {
  seq: number;
  at: number;
  type: string;
  data: Record<string, unknown>;
}

/** Event types delivered by default. `message.delta` (streamed text) is opt-in through `types`. */
export const DEFAULT_EVENT_TYPES = ['agent.start', 'agent.end', 'message', 'tool.start', 'tool.end', 'external.message', 'compaction'] as const;

export function stringParam(params: Record<string, unknown>, key: string, options: { max?: number; optional?: boolean } = {}): string | undefined {
  const value = params[key];
  if (value === undefined && options.optional) return undefined;
  if (typeof value !== 'string' || value.trim() === '') throw new RpcError(ERR.invalidParams, `${key} must be a non-empty string`);
  if (options.max && value.length > options.max) throw new RpcError(ERR.invalidParams, `${key} is longer than ${options.max} characters`);
  return value;
}

export function numberParam(params: Record<string, unknown>, key: string, range: { min: number; max: number }): number | undefined {
  const value = params[key];
  if (value === undefined) return undefined;
  if (typeof value !== 'number' || !Number.isFinite(value) || value < range.min || value > range.max) throw new RpcError(ERR.invalidParams, `${key} must be a number from ${range.min} to ${range.max}`);
  return value;
}
