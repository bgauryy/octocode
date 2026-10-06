import addFormats from 'ajv-formats';
import Ajv2020 from 'ajv/dist/2020.js';
import { z, ZodError, type ZodType } from 'zod';

const RESERVED_FLAGS = new Set([
  'help',
  'h',
  'version',
  'json',
  'no-input',
  'no-color',
]);
const RESERVED_COMMANDS = new Set(['help', 'version']);
const COMMAND_WORD = /^[A-Za-z0-9_.][A-Za-z0-9_.-]*$/;

export type FlagKind =
  | 'string'
  | 'integer'
  | 'number'
  | 'boolean'
  | 'enum'
  | 'array'
  | 'json';

export type ItemKind = 'string' | 'integer' | 'number' | 'boolean';

export interface Flag {
  property: string;
  name: string;
  kind: FlagKind;
  required: boolean;
  presence: boolean;
  partial: boolean;
  hasDefault: boolean;
  description?: string;
  defaultValue?: unknown;
  enumValues?: unknown[];
  itemKind?: ItemKind;
}

export type JsonSchema = Record<string, unknown>;

export interface ToolAnnotations {
  title?: string;
  readOnlyHint?: boolean;
  destructiveHint?: boolean;
  idempotentHint?: boolean;
  openWorldHint?: boolean;
}

export interface CliCommand {
  name: string;
  description: string;
  title?: string;
  flags: readonly Flag[];
  source: 'zod' | 'mcp';
  schema?: ZodType;
  inputSchema?: JsonSchema;
  outputSchema?: JsonSchema;
  annotations?: ToolAnnotations;
  mcpName?: string;
  fidelityNotes: readonly string[];
  run: (input: Record<string, unknown>) => Promise<unknown> | unknown;
}

export interface CliSpec {
  name: string;
  instructions: string;
  version: string;
  commands: readonly CliCommand[];
}

export interface DefineCommandInput {
  name: string;
  description: string;
  run: CliCommand['run'];
  title?: string;
  schema?: ZodType;
  inputSchema?: JsonSchema;
  outputSchema?: JsonSchema;
  annotations?: ToolAnnotations;
  source?: 'zod' | 'mcp';
  mcpName?: string;
  notes?: readonly string[];
}

interface ZodNode {
  description?: string;
  def: {
    type: string;
    innerType?: ZodNode;
    defaultValue?: unknown;
    checks?: readonly ZodCheck[];
    entries?: Record<string, unknown>;
    element?: ZodNode;
    shape?: Record<string, ZodNode>;
    options?: readonly ZodNode[];
    values?: readonly unknown[];
  };
}

interface ZodCheck {
  def?: { check?: string; format?: string };
  _zod?: { def?: { check?: string; format?: string } };
}

interface Unwrapped {
  node: ZodNode;
  required: boolean;
  hasDefault: boolean;
  defaultValue?: unknown;
  description?: string;
  notes: string[];
}

const validators = new WeakMap<object, (data: unknown) => string | undefined>();

function asNode(schema: ZodType): ZodNode {
  return schema as unknown as ZodNode;
}

function notesOf(...groups: string[][]): string[] {
  return [...new Set(groups.flat())].sort();
}

export function commandToken(name: string): { token: string; aliased: boolean } {
  if (COMMAND_WORD.test(name) && !RESERVED_COMMANDS.has(name)) {
    return { token: name, aliased: false };
  }
  let token = name.replace(/[^A-Za-z0-9_.-]+/g, '-').replace(/^-+|\.+$|-+$/g, '');
  if (!COMMAND_WORD.test(token)) token = `tool-${token || 'command'}`;
  if (RESERVED_COMMANDS.has(token)) token = `${token}-command`;
  return { token, aliased: true };
}

function kebab(property: string): string {
  const value = property
    .replace(/([a-z0-9])([A-Z])/g, '$1-$2')
    .replace(/[_\s]+/g, '-')
    .replace(/[^A-Za-z0-9-]/g, '-')
    .replace(/-+/g, '-')
    .replace(/^-|-$/g, '')
    .toLowerCase();
  return value || 'option';
}

function allocateFlag(property: string, used: Set<string>): string {
  let base = kebab(property);
  if (RESERVED_FLAGS.has(base)) base = `${base}-value`;
  let name = base;
  let index = 2;
  while (used.has(name) || RESERVED_FLAGS.has(name)) {
    name = `${base}-${index}`;
    index += 1;
  }
  used.add(name);
  return name;
}

function unwrapZod(schema: ZodNode): Unwrapped {
  let current = schema;
  let required = true;
  let hasDefault = false;
  let defaultValue: unknown;
  let description: string | undefined;
  const notes: string[] = [];
  const seen = new Set<ZodNode>();
  while (!seen.has(current)) {
    seen.add(current);
    if (description === undefined && current.description) description = current.description;
    const type = current.def.type;
    if (type === 'optional' || type === 'default' || type === 'prefault' || type === 'readonly' || type === 'catch') {
      if (type === 'optional') required = false;
      if (type === 'default' || type === 'prefault') {
        required = false;
        hasDefault = true;
        const raw = current.def.defaultValue;
        defaultValue = typeof raw === 'function' ? (raw as () => unknown)() : raw;
      }
      const inner = current.def.innerType;
      if (!inner) break;
      current = inner;
      continue;
    }
    if (type === 'nullable') {
      notes.push('nullable');
      const inner = current.def.innerType;
      if (!inner) break;
      current = inner;
      continue;
    }
    break;
  }
  return { node: current, required, hasDefault, defaultValue, description, notes };
}

function checkInfo(check: ZodCheck): { check?: string; format?: string } | undefined {
  return check.def ?? check._zod?.def;
}

function isIntegerNode(node: ZodNode): boolean {
  return (node.def.checks ?? []).some(check => {
    const info = checkInfo(check);
    return info?.format === 'safeint' || info?.check === 'int';
  });
}

function literalValue(node: ZodNode): unknown {
  return node.def.values?.[0];
}

function enumFromUnion(node: ZodNode): unknown[] | undefined {
  const options = node.def.options;
  if (!options || options.length === 0) return undefined;
  const values: unknown[] = [];
  for (const option of options) {
    const unwrapped = unwrapZod(option);
    if (unwrapped.node.def.type !== 'literal') return undefined;
    values.push(literalValue(unwrapped.node));
  }
  return values;
}

function primitiveItem(node: ZodNode): ItemKind | undefined {
  const unwrapped = unwrapZod(node);
  if (unwrapped.notes.includes('nullable')) return undefined;
  const type = unwrapped.node.def.type;
  if (type === 'string') return 'string';
  if (type === 'boolean') return 'boolean';
  if (type === 'number') return isIntegerNode(unwrapped.node) ? 'integer' : 'number';
  return undefined;
}

function flagFromZod(property: string, schema: ZodNode, name: string): { flag: Flag; notes: string[] } {
  const unwrapped = unwrapZod(schema);
  const node = unwrapped.node;
  const type = node.def.type;
  const notes = [...unwrapped.notes];
  const base = {
    property,
    name,
    required: unwrapped.required,
    presence: false,
    partial: false,
    hasDefault: unwrapped.hasDefault,
    ...(unwrapped.description ? { description: unwrapped.description } : {}),
    ...(unwrapped.hasDefault ? { defaultValue: unwrapped.defaultValue } : {}),
  };
  if (notes.includes('nullable')) {
    return { flag: { ...base, kind: 'json', partial: true }, notes };
  }
  if (type === 'string') {
    const infos = (node.def.checks ?? []).map(check => checkInfo(check));
    if (infos.some(info => info?.check === 'custom')) notes.push('refine');
    else if (infos.some(info => info !== undefined)) notes.push('constraint');
    return { flag: { ...base, kind: 'string' }, notes };
  }
  if (type === 'number') {
    return {
      flag: { ...base, kind: isIntegerNode(node) ? 'integer' : 'number' },
      notes,
    };
  }
  if (type === 'boolean') {
    const presence = !base.required && base.hasDefault && base.defaultValue === false;
    return { flag: { ...base, kind: 'boolean', presence }, notes };
  }
  if (type === 'enum') {
    return {
      flag: { ...base, kind: 'enum', enumValues: Object.values(node.def.entries ?? {}) },
      notes,
    };
  }
  if (type === 'literal') {
    return { flag: { ...base, kind: 'enum', enumValues: [literalValue(node)] }, notes };
  }
  if (type === 'union') {
    const values = enumFromUnion(node);
    if (values) return { flag: { ...base, kind: 'enum', enumValues: values }, notes };
    notes.push('union');
    return { flag: { ...base, kind: 'json', partial: true }, notes };
  }
  if (type === 'array') {
    const item = node.def.element ? primitiveItem(node.def.element) : undefined;
    if (item) return { flag: { ...base, kind: 'array', itemKind: item }, notes };
    notes.push('array');
    return { flag: { ...base, kind: 'json', partial: true }, notes };
  }
  notes.push(type || 'unknown');
  return { flag: { ...base, kind: 'json', partial: true }, notes };
}

export function flagsFromZod(schema: ZodType): { flags: Flag[]; notes: string[] } {
  const node = asNode(schema);
  const unwrapped = unwrapZod(node);
  if (unwrapped.node.def.type !== 'object' || !unwrapped.node.def.shape) {
    return {
      flags: [
        {
          property: 'input',
          name: 'input',
          kind: 'json',
          required: true,
          presence: false,
          partial: true,
          hasDefault: false,
          description: unwrapped.description,
        },
      ],
      notes: notesOf(['partial'], unwrapped.notes),
    };
  }
  const used = new Set<string>();
  const flags: Flag[] = [];
  const notes: string[] = [...unwrapped.notes];
  for (const [property, child] of Object.entries(unwrapped.node.def.shape)) {
    const built = flagFromZod(property, child, allocateFlag(property, used));
    flags.push(built.flag);
    notes.push(...built.notes);
  }
  if (flags.some(flag => flag.partial)) notes.push('partial');
  return { flags, notes: notesOf(notes) };
}

function schemaNotes(schema: JsonSchema): string[] {
  const notes: string[] = [];
  for (const keyword of ['oneOf', 'anyOf', 'allOf', '$ref', 'patternProperties'] as const) {
    if (schema[keyword] !== undefined) notes.push(keyword);
  }
  if (schema.additionalProperties !== undefined && schema.additionalProperties !== false) {
    notes.push('additionalProperties');
  }
  return notes;
}

function isPrimitiveType(value: unknown): value is ItemKind {
  return value === 'string' || value === 'integer' || value === 'number' || value === 'boolean';
}

function flagFromJsonProperty(
  property: string,
  schema: unknown,
  required: boolean,
  name: string,
): { flag: Flag; notes: string[] } {
  if (schema === true || schema === false || schema === null || typeof schema !== 'object' || Array.isArray(schema)) {
    return {
      flag: {
        property,
        name,
        kind: 'json',
        required,
        presence: false,
        partial: true,
        hasDefault: false,
      },
      notes: ['partial'],
    };
  }
  const record = schema as JsonSchema;
  const notes = schemaNotes(record);
  const hasDefault = Object.prototype.hasOwnProperty.call(record, 'default');
  const defaultValue = record.default;
  const description = typeof record.description === 'string' ? record.description : undefined;
  const flagRequired = hasDefault ? false : required;
  const base = {
    property,
    name,
    required: flagRequired,
    presence: false,
    partial: false,
    hasDefault,
    ...(description ? { description } : {}),
    ...(hasDefault ? { defaultValue } : {}),
  };
  if (notes.length > 0) {
    return { flag: { ...base, kind: 'json', partial: true }, notes };
  }
  const enumValues = Array.isArray(record.enum) ? record.enum : undefined;
  if (enumValues && enumValues.every(value => typeof value !== 'object')) {
    return { flag: { ...base, kind: 'enum', enumValues }, notes };
  }
  const type = record.type;
  if (Array.isArray(type)) {
    return { flag: { ...base, kind: 'json', partial: true }, notes: ['union'] };
  }
  if (type === 'string') return { flag: { ...base, kind: 'string' }, notes };
  if (type === 'integer') return { flag: { ...base, kind: 'integer' }, notes };
  if (type === 'number') return { flag: { ...base, kind: 'number' }, notes };
  if (type === 'boolean') {
    const presence = !flagRequired && hasDefault && defaultValue === false;
    return { flag: { ...base, kind: 'boolean', presence }, notes };
  }
  if (type === 'array') {
    const items = record.items;
    if (items && typeof items === 'object' && !Array.isArray(items)) {
      const itemType = (items as JsonSchema).type;
      if (isPrimitiveType(itemType) && schemaNotes(items as JsonSchema).length === 0) {
        return { flag: { ...base, kind: 'array', itemKind: itemType }, notes };
      }
    }
    return { flag: { ...base, kind: 'json', partial: true }, notes: ['array'] };
  }
  return { flag: { ...base, kind: 'json', partial: true }, notes: notesOf(notes, ['partial']) };
}

function isPropertyMap(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === 'object' && !Array.isArray(value);
}

function objectProperties(schema: JsonSchema): Record<string, unknown> | undefined {
  const properties = schema.properties;
  if (schema.type === 'object') {
    if (properties === undefined) return {};
    return isPropertyMap(properties) ? properties : undefined;
  }
  if (schema.type !== undefined) return undefined;
  if (isPropertyMap(properties)) return properties;
  const known = new Set(['$schema', 'additionalProperties', 'description', 'title', 'required']);
  const extra = Object.keys(schema).filter(key => !known.has(key));
  return extra.length === 0 ? {} : undefined;
}

export function flagsFromJsonSchema(schema: JsonSchema): { flags: Flag[]; notes: string[] } {
  const rootNotes = schemaNotes(schema);
  const properties = objectProperties(schema);
  if (!properties) {
    return {
      flags: [
        {
          property: 'input',
          name: 'input',
          kind: 'json',
          required: true,
          presence: false,
          partial: true,
          hasDefault: false,
        },
      ],
      notes: notesOf(rootNotes, ['partial']),
    };
  }
  const required = new Set(Array.isArray(schema.required) ? schema.required.map(String) : []);
  const used = new Set<string>();
  const flags: Flag[] = [];
  const notes = [...rootNotes];
  for (const [property, child] of Object.entries(properties as Record<string, unknown>)) {
    const built = flagFromJsonProperty(property, child, required.has(property), allocateFlag(property, used));
    flags.push(built.flag);
    notes.push(...built.notes);
  }
  if (flags.some(flag => flag.partial)) notes.push('partial');
  return { flags, notes: notesOf(notes) };
}

function assertCommandName(name: string): void {
  if (RESERVED_COMMANDS.has(name)) {
    throw new Error(`Reserved command name: ${name}`);
  }
  if (!COMMAND_WORD.test(name)) {
    throw new Error(`Command name must match ${COMMAND_WORD}: ${name}`);
  }
}

export function defineCommand(input: DefineCommandInput): CliCommand {
  assertCommandName(input.name);
  const source = input.source ?? (input.schema && !input.inputSchema ? 'zod' : 'mcp');
  if (source === 'zod' && !input.schema) {
    throw new Error(`Zod command ${input.name} requires schema`);
  }
  if (source === 'mcp' && !input.inputSchema) {
    throw new Error(`MCP command ${input.name} requires inputSchema`);
  }
  const built = source === 'zod'
    ? flagsFromZod(input.schema as ZodType)
    : flagsFromJsonSchema(input.inputSchema as JsonSchema);
  return {
    name: input.name,
    description: input.description,
    ...(input.title ? { title: input.title } : {}),
    flags: built.flags,
    source,
    ...(input.schema ? { schema: input.schema } : {}),
    ...(input.inputSchema ? { inputSchema: input.inputSchema } : {}),
    ...(input.outputSchema ? { outputSchema: input.outputSchema } : {}),
    ...(input.annotations ? { annotations: input.annotations } : {}),
    ...(input.mcpName ? { mcpName: input.mcpName } : {}),
    fidelityNotes: notesOf([...(input.notes ?? [])], built.notes),
    run: input.run,
  };
}

export function defineCli(input: {
  name: string;
  instructions: string;
  version?: string;
  commands: readonly CliCommand[];
}): CliSpec {
  const seen = new Set<string>();
  for (const command of input.commands) {
    assertCommandName(command.name);
    if (seen.has(command.name)) throw new Error(`Duplicate command: ${command.name}`);
    seen.add(command.name);
  }
  return {
    name: input.name,
    instructions: input.instructions,
    version: input.version ?? '0.0.0',
    commands: [...input.commands],
  };
}

export function withCommands(spec: CliSpec, extra: readonly CliCommand[]): CliSpec {
  return defineCli({
    name: spec.name,
    instructions: spec.instructions,
    version: spec.version,
    commands: [...spec.commands, ...extra],
  });
}

export function shellWord(value: string): string {
  if (value.length > 0 && /^[A-Za-z0-9_./:@+=,-]+$/.test(value)) return value;
  return `"${value.replace(/(["\\$`])/g, '\\$&')}"`;
}

export function flagToken(flag: Flag): string {
  if (flag.presence) return `--${flag.name}`;
  if (flag.kind === 'enum') {
    const choices = (flag.enumValues ?? []).map(value => shellWord(String(value))).join('|');
    return `--${flag.name} <${choices}>`;
  }
  if (flag.kind === 'array') return `--${flag.name} <${flag.itemKind ?? 'json'}>`;
  if (flag.kind === 'json') return `--${flag.name} <json>`;
  if (flag.kind === 'boolean') return `--${flag.name} <true|false>`;
  return `--${flag.name} <${flag.kind}>`;
}

export function usagePattern(bin: string, command: string, flags: readonly Flag[]): string {
  const ordered = [...flags.filter(flag => flag.required), ...flags.filter(flag => !flag.required)];
  const parts = [`${bin} ${command}`];
  for (const flag of ordered) {
    const token = flagToken(flag);
    parts.push(flag.required ? token : `[${token}]`);
  }
  return parts.join(' ');
}

export function commandJsonSchema(command: CliCommand): JsonSchema {
  if (command.source === 'mcp') {
    if (!command.inputSchema) throw new Error(`MCP command ${command.name} has no inputSchema`);
    return command.inputSchema;
  }
  if (!command.schema) throw new Error(`Zod command ${command.name} has no schema`);
  return z.toJSONSchema(command.schema) as JsonSchema;
}

function validatorFor(schema: JsonSchema): (data: unknown) => string | undefined {
  const cached = validators.get(schema);
  if (cached) return cached;
  const ajv = new Ajv2020({ allErrors: true, strict: false, validateSchema: false });
  addFormats(ajv);
  const validate = ajv.compile(schema);
  const run = (data: unknown): string | undefined => {
    if (validate(data)) return undefined;
    const issue = validate.errors?.[0];
    const where = issue?.instancePath ? issue.instancePath : '';
    return `${where} ${issue?.message ?? 'does not match the schema'}`.trim();
  };
  validators.set(schema, run);
  return run;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === 'object' && !Array.isArray(value);
}

function zodRootIsObject(schema: ZodType): boolean {
  const unwrapped = unwrapZod(asNode(schema));
  return unwrapped.node.def.type === 'object' && Boolean(unwrapped.node.def.shape);
}

function jsonRootIsObject(schema: JsonSchema): boolean {
  return objectProperties(schema) !== undefined;
}

export function parseCommandInput(
  command: CliCommand,
  raw: Record<string, unknown>,
): Record<string, unknown> {
  const value: Record<string, unknown> = { ...raw };
  for (const flag of command.flags) {
    if (value[flag.property] === undefined && flag.hasDefault) {
      value[flag.property] = flag.defaultValue;
    }
  }
  if (command.source === 'zod' && command.schema) {
    const payload = zodRootIsObject(command.schema) ? value : value.input;
    try {
      const parsed = command.schema.parse(payload);
      if (isRecord(parsed)) return parsed;
      return { input: parsed };
    } catch (error) {
      if (error instanceof ZodError) {
        const issue = error.issues[0];
        const path = issue?.path.join('.') ?? '';
        throw new Error(path ? `${path}: ${issue?.message}` : (issue?.message ?? error.message));
      }
      throw error;
    }
  }
  if (!command.inputSchema) throw new Error(`MCP command ${command.name} has no inputSchema`);
  const payload = jsonRootIsObject(command.inputSchema) ? value : value.input;
  const message = validatorFor(command.inputSchema)(payload);
  if (message) throw new Error(message);
  if (isRecord(payload)) return payload;
  return { input: payload };
}
