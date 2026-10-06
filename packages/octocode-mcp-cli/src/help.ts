import {
  commandJsonSchema,
  parseCommandInput,
  flagsFromJsonSchema,
  shellWord,
  usagePattern,
  type CliCommand,
  type CliSpec,
  type Flag,
  type JsonSchema,
  type ToolAnnotations,
} from './spec.js';

const ANSI = /\u001b\[[0-9;]*m/g;

export function stripAnsi(text: string): string {
  return text.replace(ANSI, '');
}

function typeWord(kind: Flag['kind'] | Flag['itemKind'] | undefined): string {
  if (kind === 'integer') return 'int';
  if (kind === 'enum') return 'string';
  return kind ?? 'json';
}

function flagHead(flag: Flag, dashed = true): string {
  const name = dashed ? `--${flag.name}` : flag.name;
  if (flag.presence) return name;
  if (flag.kind === 'array') {
    const item = flag.itemKind === 'integer' ? 'int' : flag.itemKind === 'boolean' ? 'bool' : (flag.itemKind ?? 'json');
    return `${name} ${item === 'bool' ? 'bools' : `${item}s`}`;
  }
  if (flag.kind === 'boolean') return `${name} boolean`;
  return `${name} ${typeWord(flag.kind)}`;
}

function flagTail(flag: Flag): string {
  const parts: string[] = [];
  if (flag.description) parts.push(flag.description);
  if (flag.kind === 'enum' && flag.enumValues?.length) {
    parts.push(`{${flag.enumValues.map(value => shellWord(String(value))).join('|')}}`);
  }
  if (flag.kind === 'array') parts.push('(repeat or JSON array)');
  if (flag.required) parts.push('(required)');
  if (flag.presence) parts.push('(default false)');
  else if (flag.hasDefault) parts.push(`(default ${JSON.stringify(flag.defaultValue)})`);
  return parts.join(' ');
}

function aligned(rows: Array<{ head: string; tail: string }>, indent: string): string[] {
  const width = Math.max(1, ...rows.map(row => row.head.length));
  return rows.map(row => `${indent}${row.head.padEnd(width)}   ${row.tail}`.trimEnd());
}

function asSchema(value: unknown): JsonSchema | undefined {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return undefined;
  return value as JsonSchema;
}

function schemaList(value: unknown): JsonSchema[] {
  if (!Array.isArray(value)) return [];
  return value.map(asSchema).filter((item): item is JsonSchema => item !== undefined);
}

function deref(root: JsonSchema, schema: JsonSchema, seen = new Set<string>()): JsonSchema {
  const ref = schema.$ref;
  if (typeof ref !== 'string' || !ref.startsWith('#/') || seen.has(ref)) return schema;
  seen.add(ref);
  let cursor: unknown = root;
  for (const part of ref.slice(2).split('/').map(decodeURIComponent)) {
    if (!cursor || typeof cursor !== 'object') return schema;
    cursor = (cursor as Record<string, unknown>)[part];
  }
  const next = asSchema(cursor);
  return next ? deref(root, next, seen) : schema;
}

function flagValueSchema(flag: Flag, root: JsonSchema): JsonSchema | undefined {
  if (flag.kind !== 'json') return undefined;
  if (flag.property === 'input') return root;
  return asSchema(asSchema(root.properties)?.[flag.property]);
}

function sampleJson(schema: JsonSchema, root: JsonSchema, depth = 0): unknown {
  const resolved = deref(root, schema);
  if (depth > 4) return {};
  if (resolved.const !== undefined) return resolved.const;
  if (Object.prototype.hasOwnProperty.call(resolved, 'default') && resolved.type !== 'object' && resolved.type !== 'array') {
    return resolved.default;
  }
  if (Array.isArray(resolved.enum) && resolved.enum.length > 0) return resolved.enum[0];
  const alternative = schemaList(resolved.anyOf)[0] ?? schemaList(resolved.oneOf)[0];
  if (alternative) {
    if (!asSchema(resolved.properties) && !asSchema(alternative.properties)) return sampleJson(alternative, root, depth + 1);
    const { anyOf: _anyOf, oneOf: _oneOf, ...base } = resolved;
    return sampleJson({ ...base, ...alternative,
      properties: { ...(asSchema(base.properties) ?? {}), ...(asSchema(alternative.properties) ?? {}) },
      required: [...new Set([...(Array.isArray(base.required) ? base.required : []), ...(Array.isArray(alternative.required) ? alternative.required : [])])],
    }, root, depth + 1);
  }
  const type = Array.isArray(resolved.type) ? resolved.type.find(item => item !== 'null') : resolved.type;
  if (type === 'array') {
    const items = asSchema(resolved.items);
    return items ? [sampleJson(items, root, depth + 1)] : [];
  }
  if (type === 'object' || asSchema(resolved.properties)) {
    const properties = asSchema(resolved.properties) ?? {};
    const required = Array.isArray(resolved.required) ? resolved.required.map(String) : [];
    const value: Record<string, unknown> = {};
    for (const key of required) {
      const child = asSchema(properties[key]);
      value[key] = child ? sampleJson(child, root, depth + 1) : null;
    }
    return value;
  }
  if (type === 'integer') return Math.max(1, typeof resolved.minimum === 'number' ? Math.ceil(resolved.minimum) : 1);
  if (type === 'number') return typeof resolved.minimum === 'number' ? resolved.minimum : 1.5;
  if (type === 'boolean') return true;
  if (type === 'null') return null;
  return 'value';
}

function shellJson(value: unknown): string {
  const json = JSON.stringify(value);
  if (!json.includes("'")) return `'${json}'`;
  return `"${json.replace(/(["\\$`])/g, '\\$&')}"`;
}

function exampleOf(spec: CliSpec, command: CliCommand, root: JsonSchema): string | undefined {
  const candidates = Array.isArray(root.examples) ? root.examples : [sampleJson(root, root)];
  let input: Record<string, unknown> | undefined;
  for (const candidate of candidates) {
    try {
      input = parseCommandInput(command, command.flags.length === 1 && command.flags[0]?.property === 'input' ? { input: candidate } : candidate as Record<string, unknown>);
      break;
    } catch { /* Unsupported constraints must never produce an invalid example. */ }
  }
  if (input === undefined) return undefined;
  const parts = [`${spec.name} ${command.name}`];
  for (const flag of command.flags) {
    const value = input[flag.property];
    if (value === undefined) continue;
    const token = (item: unknown) => typeof item === 'string' ? (item === 'value' ? '"value"' : shellWord(item)) : typeof item === 'object' ? shellJson(item) : String(item);
    if (flag.presence) { if (value === true) parts.push(`--${flag.name}`); }
    else if (flag.kind === 'array' && Array.isArray(value)) for (const item of value) parts.push(`--${flag.name} ${token(item)}`);
    else parts.push(`--${flag.name} ${flag.kind === 'json' ? shellJson(value) : token(value)}`);
  }
  return parts.join(' ');
}

function schemaWord(schema: JsonSchema): string {
  if (schema.const !== undefined && typeof schema.const !== 'object') return '';
  if (Array.isArray(schema.enum)) return 'string';
  const type = Array.isArray(schema.type) ? schema.type.find(item => item !== 'null') : schema.type;
  if (type === 'integer') return 'int';
  if (type === 'boolean') return 'boolean';
  if (type === 'array') {
    const itemType = asSchema(schema.items)?.type;
    if (itemType === 'string') return 'strings';
    if (itemType === 'integer') return 'ints';
    if (itemType === 'number') return 'numbers';
    if (itemType === 'boolean') return 'bools';
    return 'json';
  }
  if (type === 'object') return 'json';
  return typeof type === 'string' ? type : 'json';
}

function schemaTail(schema: JsonSchema, required: boolean): string {
  const parts: string[] = [];
  if (typeof schema.description === 'string') {
    const description = schema.description.replace(/\s+/g, ' ').trim();
    if (description) parts.push(description);
  }
  if (schema.const !== undefined && typeof schema.const !== 'object') parts.push(shellWord(String(schema.const)));
  if (Array.isArray(schema.enum)) parts.push(`{${schema.enum.map(value => shellWord(String(value))).join('|')}}`);
  if (required) parts.push('(required)');
  if (Object.prototype.hasOwnProperty.call(schema, 'default')) parts.push(`(default ${JSON.stringify(schema.default)})`);
  return parts.join(' ');
}

function indentOf(depth: number): string {
  return ' '.repeat(10 + depth * 2);
}

function outlineLines(schema: JsonSchema, root: JsonSchema, prefix: string, depth = 0): string[] {
  if (depth > 6) return [];
  const resolved = deref(root, schema);
  const alternatives = [...schemaList(resolved.anyOf), ...schemaList(resolved.oneOf)];
  if (alternatives.length > 1) {
    return alternatives.flatMap((branch, index) => [
      `${indentOf(depth)}${prefix}shape ${index + 1}`,
      ...fieldLines(branch, root, prefix, depth),
    ]);
  }
  return fieldLines(alternatives[0] ?? resolved, root, prefix, depth);
}

function nestedLines(schema: JsonSchema, root: JsonSchema, depth: number): string[] {
  if (schema.type === 'object' || asSchema(schema.properties)) return fieldLines(schema, root, '', depth + 1);
  if (schema.type === 'array') {
    const items = asSchema(schema.items);
    const item = items ? deref(root, items) : undefined;
    if (item && (item.type === 'object' || asSchema(item.properties) || item.anyOf || item.oneOf)) {
      return fieldLines(schema, root, '', depth + 1);
    }
  }
  const alternatives = [...schemaList(schema.anyOf), ...schemaList(schema.oneOf)];
  if (alternatives.length > 1) return outlineLines(schema, root, '', depth + 1);
  return [];
}

function fieldLines(schema: JsonSchema, root: JsonSchema, prefix: string, depth = 0): string[] {
  if (depth > 6) return [];
  const resolved = deref(root, schema);
  if (resolved.type === 'array') {
    const items = asSchema(resolved.items);
    if (!items) return [];
    const item = deref(root, items);
    if (item.type === 'object' || asSchema(item.properties) || item.anyOf || item.oneOf) {
      return outlineLines(item, root, `${prefix}[ ].`, depth);
    }
    const tail = schemaTail(resolved, false);
    return [`${indentOf(depth)}${prefix}${schemaWord(resolved)}${tail ? `   ${tail}` : ''}`];
  }
  const properties = asSchema(resolved.properties);
  if (!properties) return [];
  const required = new Set(Array.isArray(resolved.required) ? resolved.required.map(String) : []);
  const rows: Array<{ head: string; tail: string; children: string[] }> = [];
  for (const [key, raw] of Object.entries(properties)) {
    const child = asSchema(raw);
    if (!child) continue;
    const resolvedChild = deref(root, child);
    const word = schemaWord(resolvedChild);
    rows.push({
      head: word ? `${prefix}${key} ${word}` : `${prefix}${key}`,
      tail: schemaTail(resolvedChild, required.has(key)),
      children: nestedLines(resolvedChild, root, depth),
    });
  }
  const rendered = aligned(rows.map(row => ({ head: row.head, tail: row.tail })), indentOf(depth));
  return rows.flatMap((row, index) => [rendered[index] ?? '', ...row.children]);
}

export interface CommandHelpJson {
  name: string;
  description: string;
  usage: string;
  flags: readonly Flag[];
  schema: JsonSchema;
  inputSchema: JsonSchema;
  title?: string;
  outputSchema?: JsonSchema;
  annotations?: ToolAnnotations;
}

export interface RootHelpJson {
  instructions: string;
  commands: Array<{ name: string; description: string; title?: string }>;
}

function finish(text: string): string {
  const body = text.endsWith('\n') ? text : `${text}\n`;
  return stripAnsi(body);
}

function outputLines(schema: JsonSchema): string[] {
  const built = flagsFromJsonSchema(schema);
  if (built.flags.length === 1 && built.flags[0]?.property === 'input') return ['      json'];
  if (built.flags.length === 0) return ['      (none)'];
  const rows = built.flags.map(flag => {
    const shown = { ...flag, name: flag.property };
    return {
      head: flagHead(shown, false),
      tail: flagTail(shown),
      nested: flag.partial ? outlineLines(flagValueSchema(flag, schema) ?? {}, schema, '') : [],
    };
  });
  const rendered = aligned(rows.map(row => ({ head: row.head, tail: row.tail })), '      ');
  return rows.flatMap((row, index) => [rendered[index] ?? '', ...row.nested]);
}

function hintLines(annotations: ToolAnnotations | undefined): string[] {
  if (!annotations) return [];
  const lines: string[] = [];
  if (annotations.readOnlyHint !== undefined) lines.push(annotations.readOnlyHint ? 'read-only' : 'writes');
  if (annotations.destructiveHint !== undefined) lines.push(annotations.destructiveHint ? 'destructive' : 'additive');
  if (annotations.idempotentHint !== undefined) lines.push(annotations.idempotentHint ? 'idempotent' : 'not-idempotent');
  if (annotations.openWorldHint !== undefined) lines.push(annotations.openWorldHint ? 'open-world' : 'closed-world');
  return lines;
}

function sentence(text: string): string {
  const flat = text.replace(/\s+/g, ' ').trim();
  const end = flat.search(/[.!?](\s|$)/);
  return end === -1 ? flat : flat.slice(0, end + 1);
}

export function commandIndexLine(command: CliCommand, width = command.name.length + 1): string {
  const blurb = sentence(command.description);
  const label = command.title && command.title !== command.name ? command.title : '';
  const detail = label && blurb && label !== blurb ? `${label} — ${blurb}` : (blurb || label);
  const name = `${command.name}:`.padEnd(width);
  return detail ? `  ${name}  ${detail}` : `  ${name}`;
}

function commandLines(commands: readonly CliCommand[]): string[] {
  const width = Math.max(1, ...commands.map(command => command.name.length + 1));
  return commands.map(command => commandIndexLine(command, width));
}

export function commandHelp(spec: CliSpec, command: CliCommand): string {
  const root = commandJsonSchema(command);
  const usage = usagePattern(spec.name, command.name, command.flags);
  const lines = [command.description];
  if (command.title && command.title !== command.description) lines.push('', command.title);
  const flagRows = command.flags.map(flag => ({
    head: flagHead(flag),
    tail: flagTail(flag),
    nested: flag.partial ? outlineLines(flagValueSchema(flag, root) ?? {}, root, '') : [],
  }));
  const renderedFlags = aligned(flagRows.map(row => ({ head: row.head, tail: row.tail })), '      ');
  const flags = flagRows.length > 0
    ? flagRows.flatMap((row, index) => [renderedFlags[index] ?? '', ...row.nested])
    : ['      (none)'];
  lines.push('', 'USAGE', `  ${usage}`, '', 'FLAGS', ...flags);
  if (command.outputSchema) lines.push('', 'OUTPUT', ...outputLines(command.outputSchema));
  const hints = hintLines(command.annotations);
  if (hints.length > 0) lines.push('', 'HINTS', ...hints.map(hint => `  ${hint}`));
  const example = exampleOf(spec, command, root);
  if (example) lines.push('', 'EXAMPLES', `  $ ${example}`);
  return finish(lines.join('\n'));
}

export function commandHelpJson(spec: CliSpec, command: CliCommand): CommandHelpJson {
  const schema = commandJsonSchema(command);
  return {
    name: command.mcpName ?? command.name,
    description: command.description,
    usage: usagePattern(spec.name, command.name, command.flags),
    flags: command.flags,
    schema,
    inputSchema: schema,
    ...(command.title ? { title: command.title } : {}),
    ...(command.outputSchema ? { outputSchema: command.outputSchema } : {}),
    ...(command.annotations ? { annotations: command.annotations } : {}),
  };
}

export function defaultOutput(spec: CliSpec): string {
  const lines = [
    spec.instructions,
    '',
    'USAGE',
    `  ${spec.name} <command> [flags]`,
    '',
    'COMMANDS',
    ...commandLines(spec.commands),
  ];
  return finish(lines.join('\n'));
}

export function rootHelp(spec: CliSpec): string {
  const lines = [
    spec.instructions,
    '',
    'USAGE',
    `  ${spec.name} <command> [flags]`,
    '',
    'COMMANDS',
    ...commandLines(spec.commands),
    '',
    'FLAGS',
    ...aligned([
      { head: '-h, --help', tail: 'Show help for command' },
      { head: '--version', tail: 'Show version' },
      { head: '--json', tail: 'Print JSON' },
      { head: '--no-input', tail: 'Do not read stdin' },
      { head: '--no-color', tail: 'Strip color' },
    ], '      '),
    '',
    'EXAMPLES',
    `  $ ${spec.name} <command> --help`,
    '',
    'LEARN MORE',
    `  Use '${spec.name} <command> --help' for more information about a command.`,
  ];
  return finish(lines.join('\n'));
}

export function rootHelpJson(spec: CliSpec): RootHelpJson {
  return {
    instructions: spec.instructions,
    commands: spec.commands.map(command => ({
      name: command.name,
      description: command.description,
      ...(command.title ? { title: command.title } : {}),
    })),
  };
}
