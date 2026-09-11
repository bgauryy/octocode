import { parseArgs } from './command-parser.js';
import { getAwarenessCommandDescriptor } from './schema/cli.js';
import type { AwarenessCommandCall } from './command-contracts.js';

/** Decode CLI continuation metadata at the package boundary. */
export function awarenessContinuationCall(command: string, args: string[]): AwarenessCommandCall {
  const descriptor = getAwarenessCommandDescriptor(command);
  if (!descriptor) throw new Error(`Unknown continuation command: ${command}`);
  const { _: positionals, db: _db, db_scope: _scope, compact: _compact, ...params } = parseArgs(args);
  const properties = descriptor.inputSchema.properties as Record<string, Record<string, unknown>>;
  // Canonical CLI selectors can be redundant once a continuation is decoded
  // back to its uniquely bound legacy library command.
  if (
    command === 'work show' &&
    (params.kind === 'presence' || (Array.isArray(params.kind) && params.kind.length === 1 && params.kind[0] === 'presence'))
  ) delete params.kind;
  for (const [key, value] of Object.entries(params)) {
    const property = properties[key];
    if (!property) continue;
    const types = [property.type, ...((property.anyOf as Record<string, unknown>[] | undefined)?.map(item => item.type) ?? [])];
    if (types.includes('integer') || types.includes('number')) params[key] = Number(value) as never;
    else if (Array.isArray(value) && !types.includes('array') && value.length === 1) params[key] = value[0]!;
  }
  for (const [i, field] of (descriptor.positionals ?? []).entries()) if (positionals[i] !== undefined) params[field] = positionals[i]!;
  if (command === 'query' && positionals[0]) params.view = positionals[0];
  return { command, params };
}

/** Library continuations are executable request objects; shell output retains argv. */
export function structuredAwarenessContinuations(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(structuredAwarenessContinuations);
  if (!value || typeof value !== 'object') return value;
  return Object.fromEntries(Object.entries(value).map(([key, child]) => [
    key,
    ['next', 'actions', 'continuations'].includes(key) ? structuredContinuation(child)
      : key === 'rows' ? structuredAwarenessContinuations(child) : child,
  ]));
}

function structuredContinuation(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(structuredContinuation);
  if (!value || typeof value !== 'object') return value;
  const object = value as Record<string, unknown>;
  if (object.operation === 'agent_signal' && object.request && typeof object.request === 'object') {
    const request = object.request as Record<string, unknown>;
    const action = request.action;
    const command = action === 'ack' ? 'signal ack' : action === 'reply' ? 'signal reply' : undefined;
    if (command) {
      const { action: _action, ...params } = request;
      const { operation: _operation, request: _request, ...metadata } = object;
      return { ...metadata, call: { command, params } };
    }
  }
  if (typeof object.name === 'string' && Array.isArray(object.args) && object.args.every(arg => typeof arg === 'string') && getAwarenessCommandDescriptor(object.name)) {
    return awarenessContinuationCall(object.name, object.args as string[]);
  }
  if (Array.isArray(object.argv) && object.argv.every(arg => typeof arg === 'string')) {
    const argv = object.argv as string[];
    const name = argv.slice(0, 2).join(' ');
    if (getAwarenessCommandDescriptor(name)) {
      const { argv: _argv, ...rest } = object;
      // History's domain/CLI continuation carries argv plus convenience fields.
      // The command API has one executable representation, bound to its context.
      if (name.startsWith('history ') && rest.command === name) {
        delete rest.command;
        delete rest.args;
        delete rest.db;
      }
      return { ...structuredContinuation(rest) as object, call: awarenessContinuationCall(name, argv.slice(2)) };
    }
  }
  return Object.fromEntries(Object.entries(object).map(([key, child]) => [key, structuredContinuation(child)]));
}
