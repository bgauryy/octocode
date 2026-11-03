/** Worker capability families, projected from the operation catalog rather than copied schemas. */
import type { CommandInputs, CommandResults } from './catalog-types.generated.js';
import { ValidationError } from './errors.js';
import { asObject, isObject, type Json, type JsonObject } from './validation.js';

type Operation = keyof CommandInputs;
type Tool = CommandResults['schema']['tools'][number];
const GROUPS = [
  { name: 'agents', description: 'Discover peers, advertise task/status, and subscribe to topics.', variants: { list: 'peers', status: 'set_status', subscribe: 'subscribe' } },
  { name: 'messages', description: 'Exchange requests, notices and final replies.', variants: { read: 'inbox', send: 'send_message', complete: 'complete' } },
  { name: 'leases', description: 'Reserve paths and verify edit ownership in this worktree.', variants: { acquire: 'lock_many', check: 'check_write', list: 'locks', renew: 'renew', release: 'unlock' } },
  { name: 'documents', description: 'Publish, discover and read immutable evidence.', variants: { publish: 'share_document', find: 'context', read: 'read_document' } },
  { name: 'records', description: 'Repository history and persistent coordination records.', variants: { read: 'fetch', write: 'record', activity: 'activity' } },
] as const;

const MESSAGING: readonly Operation[] = ['peers', 'set_status', 'subscribe', 'inbox', 'send_message', 'complete', 'fetch'];
const REVIEW: readonly Operation[] = [...MESSAGING, 'share_document', 'context', 'read_document', 'record'];
const PROFILES: Readonly<Record<string, readonly Operation[]>> = {
  messaging: MESSAGING,
  review: REVIEW,
  editing: [...REVIEW, 'lock_many', 'check_write', 'locks', 'renew', 'unlock'],
};

export function isTool(name: string): boolean { return GROUPS.some(group => group.name === name); }

export function groupedTools(operations: readonly Tool[], selection?: string | null): Tool[] {
  let groups: readonly string[] = GROUPS.map(group => group.name);
  const profile = selection == null ? undefined : PROFILES[selection];
  if (selection != null && !profile) {
    groups = selection.split(',');
    if (new Set(groups).size !== groups.length) throw new ValidationError('Duplicate selected tool');
    for (const name of groups) if (!isTool(name)) throw new ValidationError('Unknown selected tool: ' + name + '; use schema tools');
  }
  return GROUPS.filter(group => groups.includes(group.name)).flatMap(group => {
    const variants = Object.entries(group.variants).filter(([, operation]) => !profile || profile.includes(operation));
    if (!variants.length) return [];
    const sources = variants.map(([, operation]) => {
      const found = operations.find(tool => tool.name === operation);
      if (!found) throw new ValidationError('Missing worker operation: ' + operation);
      return found;
    });
    return [{ name: group.name, description: group.description,
      annotations: { readOnlyHint: sources.every(source => source.annotations.readOnlyHint), destructiveHint: false, openWorldHint: false },
      inputSchema: { type: 'object', properties: {
        type: { type: 'string', enum: variants.map(([type]) => type), description: 'Enabled operation.' },
        input: { type: 'object', description: 'Arguments for this type; {} when none.' },
      }, required: ['type', 'input'], additionalProperties: false,
      oneOf: variants.map(([type], index) => ({ properties: { type: { const: type }, input: { ...sources[index].inputSchema, description: sources[index].description } } })),
    } }];
  });
}

export function operationFor(name: string, type: unknown): Operation {
  const group = GROUPS.find(item => item.name === name);
  if (!group || typeof type !== 'string') throw new ValidationError('Use a discovered tool with {type,input}');
  const operation = Object.entries(group.variants).find(([kind]) => kind === type)?.[1];
  if (!operation) throw new ValidationError('Unknown type for ' + name + ': ' + type);
  return operation;
}

export function toolContinuation(candidate: Json): Json {
    if (!isObject(candidate) || typeof candidate.command !== 'string' || !isObject(candidate.input)) return candidate;
    for (const group of GROUPS) for (const [type, command] of Object.entries(group.variants)) {
      if (command === candidate.command) return { command: group.name, input: { type, input: candidate.input } };
    }
    throw new ValidationError('Continuation has no worker tool: ' + candidate.command);
}

/** Translate only runtime-owned continuation locations; arbitrary record/document evidence stays untouched. */
export function groupedResult(operation: string, value: unknown): Json {
  if (!isObject(value)) throw new ValidationError('Tool result must be an object');
  const result: JsonObject = { ...value };
  if (result.next !== undefined) result.next = toolContinuation(result.next);
  if (operation === 'fetch' && Array.isArray(result.items)) result.items = result.items.map(item => {
    if (!isObject(item) || item.type !== 'message' || !isObject(item.data) || item.data.contentOmitted !== true || item.data.next === undefined) return item;
    return { ...item, data: { ...item.data, next: toolContinuation(item.data.next) } };
  });
  return result;
}

/** Whether this typed call needs the transport's stable per-call idempotency key. */
export function keyedInput(name: string, value: unknown, key: string): unknown {
  const input = retryKeyPayload(name, value);
  if (!input || 'key' in input || !isObject(value)) return value;
  return { ...value, input: { ...input, key } };
}

export function retryKeyPayload(name: string, value: unknown): JsonObject | null {
  if (!isTool(name) || !isObject(value) || !isObject(value.input)) return null;
  return ['send_message', 'record'].includes(operationFor(name, value.type)) ? value.input : null;
}

export function variantInput(value: unknown): JsonObject {
  return asObject(asObject(value, 'tool arguments').input, 'input');
}
