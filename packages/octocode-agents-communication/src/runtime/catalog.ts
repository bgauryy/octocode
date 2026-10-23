/** One catalog supplies validation, command help, MCP and host tool discovery. */
import operatingSkill from '../../OPERATING.md';
import rawCatalog from '../../assets/catalog.json';
import * as database from './database.js';
import { ValidationError } from './errors.js';
import { isBlank } from './text.js';
import { DAY_MS } from './timing.js';
import * as validation from './validation.js';
import type { Budget, CommandInputs, CommandResults, Continuation } from './catalog-types.generated.js';
import { recordVariants } from './record-variants.js';
import { compact } from './json-text.js';
import { asObject, asObjects, isObject, type Json, type JsonObject, type Schema } from './validation.js';

/** A catalog command that takes input, by name. */
export type CommandName = keyof CommandInputs;
/** One command with the input type the catalog gives it; with a union name, the union of its commands (narrow on `name`). */
export type CommandCall<N extends CommandName = CommandName> = { [M in N]: { name: M; input: CommandInputs[M] } }[N];

export const PAGE_BYTES = 16 * 1024;
/** The `messageKinds` marker of one send_message kind: its input field, whether it is a request, and whether it wakes by default. */
export interface MessageKind { readonly kind: string; readonly field: string; readonly request: boolean; readonly wake: boolean }

const clone = <T>(value: T): T => structuredClone(value);

function resolveRefs(value: Json, definitions: JsonObject, refs: readonly string[] = []): Json {
  if (isObject(value)) {
    if ('$ref' in value) {
      const ref = value.$ref;
      if (typeof ref !== 'string' || !ref.startsWith('#/$defs/') || Object.keys(value).some(key => key !== '$ref' && key !== 'description')) throw new ValidationError('Unsupported catalog reference');
      if (refs.includes(ref)) throw new ValidationError('Circular catalog reference: ' + ref);
      const result = resolveRefs(definitions[ref.slice('#/$defs/'.length)]!, definitions, [...refs, ref]) as JsonObject;
      return 'description' in value ? { ...result, description: value.description! } : result;
    }
    return Object.fromEntries(Object.entries(value).map(([key, item]) => [key, resolveRefs(item, definitions, refs)]));
  }
  return Array.isArray(value) ? value.map(item => resolveRefs(item, definitions, refs)) : value;
}

/** The whole catalog as `schema` prints it, after `$ref`s are resolved. */
type Dump = CommandResults['schema'];
type CommandEntry = Dump['commands'][number];
type RecordTypeEntry = Dump['recordTypes'][number];

let loaded: Dump | undefined;

function load(): Dump {
  if (loaded) return loaded;
  const parsed: unknown = clone(rawCatalog);
  if (!isObject(parsed)) throw new ValidationError('catalog.json must be an object');
  const { $defs, ...raw } = parsed;
  const value = asObject(resolveRefs(raw, asObject($defs, 'catalog $defs')), 'catalog');
  const commands = asObjects(value.commands, 'commands');
  const names = new Set(commands.map(item => String(item.name)));
  const recordTypes = asObjects(value.recordTypes, 'recordTypes');
  const record = asObject(commands.find(item => item.name === 'record'), 'record command');
  asObject(record.inputSchema, 'record inputSchema').oneOf = recordVariants(recordTypes.map(item => ({ type: String(item.type), writer: String(item.writer), dataSchema: item.dataSchema }))).map(variant => asObject(variant, 'record variant'));
  for (const item of commands) if ('inputSchema' in item) validation.checkSchema(item.inputSchema);
  // Every command states what it prints; `schema` carries these, tools/list and `schema <command>` do not.
  const results = asObject(value.results, 'results');
  for (const name of names) if (!Object.hasOwn(results, name)) throw new ValidationError('Command without a result schema: ' + name);
  for (const [name, schema] of Object.entries(results)) {
    if (!names.has(name)) throw new ValidationError('Result schema of an unknown command: ' + name);
    validation.checkSchema(schema);
  }
  for (const tool of asObjects(value.tools, 'tools')) {
    const source = asObject(commands.find(item => item.name === tool.name), 'tool command');
    Object.assign(tool, { description: source.description, inputSchema: clone(source.inputSchema) });
  }
  for (const item of recordTypes) {
    validation.checkSchema(item.dataSchema);
    validation.validate(asObject(item.dataSchema, 'record dataSchema'), item.example);
  }
  value.database = JSON.parse(JSON.stringify({ ...clone(database.schema()), sql: database.SQL, leasePathComparison: {
    algorithm: 'Unicode canonical caseless per component (NFD, full casefold, NFD)',
    unicodeVersion: [16, 0, 0], normalizationUnicodeVersion: [16, 0, 0],
    policy: 'Case and normalization aliases conflict on every filesystem; access paths and workspace containment stay case-preserving.' } }));
  value.pagination = { maxItems: 100, targetBytes: PAGE_BYTES, continuation: 'Run next.command with next.input unchanged. A single oversized row remains intact with an explicit budget diagnostic.' };
  // The one place the loaded JSON becomes the generated type: the dump must satisfy the `schema` result schema.
  validation.validate(asObject(results.schema, 'schema result'), value);
  const dump: unknown = value;
  return (loaded = dump as Dump);
}
function find(name: string): CommandEntry {
  const item = load().commands.find(command => command.name === name);
  if (!item) throw new ValidationError('Unknown command: ' + name + '; use --help');
  return item;
}

/** The input schema of a command that takes input. */
function inputSchema(name: string): JsonObject {
  return asObject(find(name).inputSchema, `${name} inputSchema`);
}

/** The schema of one input property of a command. */
export function inputProperty(command: string, property: string): JsonObject {
  return clone(asObject(asObject(inputSchema(command).properties, `${command} properties`)[property], `${command} ${property}`));
}

export function catalog(): Dump {
  return clone(load());
}

export function definition(name: string): CommandEntry {
  return clone(find(name));
}

export function recordType(name: string): RecordTypeEntry {
  const item = load().recordTypes.find(type => type.type === name);
  if (!item) throw new ValidationError('Unknown record type: ' + name);
  return clone(item);
}

/** The field inventory of one record type (or nested object): which fields are required, optional, generic or nested. */
export interface DataFields {
  required: string[];
  optional: string[];
  generic: Json;
  nested?: { [field: string]: DataFields };
  variants?: Json;
}

function dataFields(schema: JsonObject): DataFields {
  const properties = asObject(schema.properties, 'record data properties');
  const required = Array.isArray(schema.required) ? schema.required.map(String) : [];
  const result: DataFields = { required, optional: Object.keys(properties).filter(key => !required.includes(key)), generic: schema.additionalProperties ?? false };
  const nested = Object.entries(properties).flatMap(([key, value]) => isObject(value) && 'properties' in value ? [[key, dataFields(value)] as const] : []);
  if (nested.length) result.nested = Object.fromEntries(nested);
  if ('oneOf' in schema) result.variants = schema.oneOf;
  return result;
}

/** Complete field inventory, without full schemas/examples or SQL. */
export function typeSummary(): (DataFields & { type: string; writer: string })[] {
  return clone(load().recordTypes.map(item => ({ type: item.type, ...dataFields(asObject(item.dataSchema, 'record dataSchema')), writer: item.writer })));
}

const BYTE_UNITS: ReadonlySet<string> = new Set(['reasoning', 'content']);
const LIMITS: Readonly<Record<string, number>> = { reasoning: 512, content: 1024 * 1024, body: 16384, question: 16384, path: 4096 };
const STORED_KEYS: ReadonlySet<string> = new Set(['reasoning', 'content', 'body', 'question', 'path']);

/** Stored text is bounded in UTF-8 bytes (reasoning, content) or UTF-16 units (everything else); `prompt` is unbounded. */
function length(key: string, value: string): void {
  if (key === 'prompt') return;
  const bytes = BYTE_UNITS.has(key);
  const limit = LIMITS[key] ?? 256;
  const used = bytes ? Buffer.byteLength(value, 'utf8') : value.length;
  if (used > limit) {
    const maximum = limit % (1024 * 1024) === 0 ? `${limit / (1024 * 1024)} MiB` : String(limit);
    throw new ValidationError(`Invalid ${key}: ${used} ${bytes ? 'UTF-8 bytes' : 'UTF-16 units'} exceeds ${maximum}`);
  }
}

export function storedLengths(value: unknown): void {
  if (Array.isArray(value)) for (const item of value) storedLengths(item);
  else if (isObject(value)) for (const [key, item] of Object.entries(value)) {
    if (typeof item === 'string' && STORED_KEYS.has(key)) length(key, item);
    else storedLengths(item);
  }
}

export function validate(schema: Schema, value: Json): void {
  storedLengths(value);
  validation.validate(schema, value);
}

/** Every send_message kind with its markers, in catalog order (shared and read-only, like the catalog it comes from). */
export function messageKinds(): readonly MessageKind[] {
  return load().messageKinds;
}

/** The one message-kind field present in a send_message input: `body` is a `note`. */
export function kindOf(input: object): string {
  const kinds = load().messageKinds;
  const present = kinds.filter(spec => spec.field in input);
  if (present.length !== 1) throw new ValidationError('Supply exactly one message kind: ' + kinds.map(spec => spec.field).join(', '));
  return present[0]!.kind;
}

/** The catalog command `name` that takes input, else a validation error. */
export function commandName(name: string): CommandName {
  if (!('inputSchema' in find(name))) throw new ValidationError('Command takes no input: ' + name);
  return name as CommandName;
}

/** The record types the `record` command accepts: the others have a dedicated writer. */
function writableRecordTypes(): unknown[] {
  const types = asObject(asObject(inputSchema('record').properties, 'record properties').type, 'record type').enum;
  return Array.isArray(types) ? types : [];
}

/**
 * Validates a command's input (the catalog schema, stored lengths, and the cross-field rules a schema cannot state) and
 * returns it typed by the generated catalog shapes. The generated types omit those cross-field rules, so validation here,
 * not the type, is what makes the input trustworthy.
 */
export function command<N extends CommandName>(name: N, value: unknown): CommandCall<N> {
  const input = isObject(value) ? value : undefined;
  if (name === 'record' && input && writableRecordTypes().includes(input.type) && 'data' in input) validation.validate(asObject(recordType(String(input.type)).dataSchema, 'record dataSchema'), input.data);
  if (name === 'send_message' && input && 'replyTo' in input) throw new ValidationError('Replies use complete {message, outcome, reply} only. For progress send a delta with to and conversationId; omit replyTo.');
  if (name === 'send_message' && input && ('to' in input) === ('topic' in input)) throw new ValidationError('Supply exactly one recipient: to or topic');
  if (name === 'send_message' && input) kindOf(input);
  if (name === 'complete' && input && 'reasoning' in input && !('reply' in input)) throw new ValidationError('Invalid complete: to handle without replying, omit reasoning and use message or messages only. Do not add a reply merely to satisfy validation; handled informational messages need no reply.');
  if (name === 'complete' && input && ('outcome' in input || 'reply' in input) && ('messages' in input || !('reply' in input && 'outcome' in input && 'message' in input))) throw new ValidationError('Invalid complete: a request answer needs one message, its outcome and reply: {message, outcome, reply}. Complete FYIs with message or messages and no outcome; leave unfinished work pending.');
  const item = find(name);
  if (name === 'record' && input) {
    const { data: _data, ...rest } = input;
    storedLengths(rest);
  } else storedLengths(value);
  if (item.inputSchema !== undefined) validation.validate(asObject(item.inputSchema, `${name} inputSchema`), value);
  return { name, input: value } as CommandCall<N>;
}

/** A required non-blank string field, bounded like stored text. */
export function text<K extends string>(value: { readonly [P in K]?: unknown }, key: K): string {
  const result = value[key];
  if (typeof result !== 'string') throw new ValidationError('Missing string: ' + key);
  if (isBlank(result)) throw new ValidationError('Invalid ' + key + ': blank');
  length(key, result);
  return result;
}

export function ttl(value: { readonly ttlMs?: unknown }, defaultValue: number, maximum = DAY_MS): number {
  const result = value.ttlMs ?? defaultValue;
  if (typeof result !== 'number' || !Number.isInteger(result) || result < 1000 || result > maximum) throw new ValidationError('Invalid TTL');
  return result;
}

const PRESETS: Readonly<Record<string, string>> = {
  messaging: 'peers,set_status,send_message,notify_all,inbox,complete,fetch',
  review: 'peers,set_status,send_message,notify_all,inbox,complete,fetch,record,share_document,read_document,context',
  editing: 'peers,set_status,send_message,notify_all,inbox,complete,fetch,record,share_document,read_document,context,locks,lock,lock_many,renew,unlock',
};

export function selectedTools(selection?: string | null): Dump['tools'] {
  const tools = load().tools;
  if (selection == null) return clone(tools);
  const names = (Object.hasOwn(PRESETS, selection) ? PRESETS[selection]! : selection).split(',');
  const available = new Set(tools.map(tool => tool.name));
  for (const name of names) if (!available.has(name)) throw new ValidationError('Unknown selected tool: ' + name + '; use schema to inspect tools');
  if (names.length !== new Set(names).size) throw new ValidationError('Duplicate selected tool');
  return clone(tools.filter(tool => names.includes(tool.name)));
}

export function deliveryBatchLimit(): number {
  const limit = inputProperty('confirm_delivery', 'items').maxItems;
  if (typeof limit !== 'number') throw new ValidationError('confirm_delivery items has no maxItems');
  return limit;
}

function indexOf(value: string, needle: string, from = 0): number {
  const index = value.indexOf(needle, from);
  if (index < 0) throw new ValidationError('Skill instructions lack ' + needle);
  return index;
}

export function skillInstructions(vendor?: string | null): string {
  return vendor == null ? operatingSkill : operatingSkill.slice(indexOf(operatingSkill, '# '));
}

export function workerSkill(selection?: string | null): string {
  let value = skillInstructions('worker');
  const names = new Set(selectedTools(selection).map(tool => tool.name));
  if (!names.has('lock') && !names.has('lock_many')) {
    let start = indexOf(value, '## Edit with ownership');
    let end = indexOf(value, '\n## ', start + 3);
    value = value.slice(0, start) + value.slice(end + 1);
    start = indexOf(value, '```mermaid');
    end = indexOf(value, '## Discover and choose work', start);
    value = value.slice(0, start) + value.slice(end);
  }
  return value;
}

export function help() {
  return {
    package: '@octocodeai/octocode-agents-communication', implementation: 'TypeScript',
    usage: 'npx -y @octocodeai/octocode-agents-communication <command> [json|-] --workspace <path> [--database <file>] [--session <id>]',
    commands: load().commands.map(item => item.name),
    discover: ['skill', '<command> --help', 'schema types --compact', 'schema type <name>', 'db info'],
    toolProfiles: { messaging: 'messages/status', review: 'messaging + documents/context', editing: 'review + leases' },
  };
}

/** One page of rows under the item and byte budgets, with a `next` continuation when rows remain. */
export function page<T extends { id: number | string } | { recordId: number }>(rows: readonly T[], stringCursor: boolean, commandName: string, input: object, metadataBytes = 0): { items: T[]; next?: Continuation; budget?: Budget } {
  let size = 64 + metadataBytes;
  let count = 0;
  for (const row of rows.slice(0, 100)) {
    const added = Buffer.byteLength(compact(row)) + 1;
    if (count > 0 && size + added > PAGE_BYTES) break;
    size += added;
    count += 1;
  }
  const result: { items: T[]; next?: Continuation; budget?: Budget } = { items: rows.slice(0, count) };
  if (rows.length > count && count) {
    const last = rows[count - 1]!;
    const cursor = 'recordId' in last ? last.recordId : last.id;
    result.next = { command: commandName, input: { ...input, after: stringCursor ? String(cursor) : cursor } };
  }
  if (size > PAGE_BYTES) result.budget = { targetBytes: PAGE_BYTES, reason: 'Single oversized row returned intact to preserve evidence and cursor progress' };
  return result;
}
