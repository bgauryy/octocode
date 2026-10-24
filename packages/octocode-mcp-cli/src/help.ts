import { flagsFromJsonSchema } from './json-schema-flags.js';
import {
  commandJsonSchema,
  parseCommandInput,
  type CliCommand,
  type CliSpec,
  type Flag,
  type JsonSchema,
  type ToolAnnotations,
} from './spec.js';
import { asSchema, deref, schemaList } from './json-schema.js';
import { aligned, outlineLines } from './schema-outline.js';
import { shellWord, usagePattern } from './usage.js';

const ANSI = /\u001b\[[0-9;]*m/g;

export function stripAnsi(text: string): string {
  return text.replace(ANSI, '');
}

function typeWord(kind: Flag['kind'] | Flag['itemKind'] | undefined): string {
  if (kind === 'integer') return 'int';
  if (kind === 'enum') return 'string';
  return kind ?? 'json';
}

function flagHead(flag: Flag, dashed: boolean): string {
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
  const members = schemaList(resolved.allOf);
  if (members.length > 0) {
    // Fold each member's first alternative into one object schema; the sample must satisfy every member.
    const { allOf: _allOf, ...merged } = resolved;
    for (const member of members) {
      const part = schemaList(member.anyOf)[0] ?? schemaList(member.oneOf)[0] ?? member;
      merged.properties = { ...(asSchema(merged.properties) ?? {}), ...(asSchema(part.properties) ?? {}) };
      merged.required = [...new Set([...(Array.isArray(merged.required) ? merged.required : []), ...(Array.isArray(part.required) ? part.required : [])])];
    }
    return sampleJson(merged, root, depth + 1);
  }
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

/** One aligned line per flag, each partial flag followed by its schema outline. */
function flagLines(flags: readonly Flag[], root: JsonSchema, dashed: boolean): string[] {
  if (flags.length === 0) return ['      (none)'];
  const rows = flags.map(flag => {
    const shown = dashed ? flag : { ...flag, name: flag.property };
    return {
      head: flagHead(shown, dashed),
      tail: flagTail(shown),
      nested: flag.partial ? outlineLines(flagValueSchema(flag, root) ?? {}, root, '') : [],
    };
  });
  const rendered = aligned(rows.map(row => ({ head: row.head, tail: row.tail })), '      ');
  return rows.flatMap((row, index) => [rendered[index] ?? '', ...row.nested]);
}

function outputLines(schema: JsonSchema): string[] {
  const built = flagsFromJsonSchema(schema);
  if (built.flags.length === 1 && built.flags[0]?.property === 'input') return ['      json'];
  return flagLines(built.flags, schema, false);
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

function commandIndexLine(command: CliCommand, width = command.name.length + 1): string {
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
  lines.push('', 'USAGE', `  ${usage}`, '', 'FLAGS', ...flagLines(command.flags, root, true));
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

function overviewLines(spec: CliSpec): string[] {
  return [
    spec.instructions,
    '',
    'USAGE',
    `  ${spec.name} <command> [flags]`,
    '',
    'COMMANDS',
    ...commandLines(spec.commands),
  ];
}

export function defaultOutput(spec: CliSpec): string {
  return finish(overviewLines(spec).join('\n'));
}

export function rootHelp(spec: CliSpec): string {
  const lines = [
    ...overviewLines(spec),
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
