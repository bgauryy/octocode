// CLI flags for tool commands, generated from each tool's canonical query
// schema so the CLI never spells a field list of its own. Every field is one
// long flag named exactly as the field. The authored additions (short
// letters, positionals, examples) live in cli-authored.ts; generation fails
// when any of them names a field or value the schema lacks.
// Help text the schema leaves out comes from core (`cliHelp`).

import {
  CLI_FIELD_FALLBACKS,
  CLI_FIELD_NOTES,
  CLI_TOOL_OUTPUT,
} from '@octocodeai/octocode-core/schema';
import { AUTHORED, type CliPositional } from './cli-authored.ts';

type Json = Record<string, unknown>;

/** How the CLI reads a flag's text into the field's JSON value. */
export type FlagKind =
  | 'bool'
  | 'string'
  | 'integer'
  | 'number'
  | 'enum'
  | 'strings'
  | 'integers'
  | 'json';

export interface CliFlag {
  field: string;
  kind: FlagKind;
  /** Allowed values of an `enum` (or an enum array's items). */
  values?: string[];
  /** In the row MCP publishes; secondary fields print under their own heading. */
  primary: boolean;
  description: string;
  short?: string;
  /** A list whose items never hold a comma also takes `a,b`, as gh does. */
  commas?: true;
}

/** A short letter that sets one field to one value (`-i` → `caseMode: insensitive`). */
export interface CliPreset {
  short: string;
  field: string;
  value: string;
  description: string;
}

/** A short letter whose `A/B` value fills two fields (`-R owner/repo`). */
export interface CliSplit {
  short: string;
  fields: [string, string];
  separator: string;
  valueName: string;
  description: string;
}

export interface CliSpec {
  /** Set when a first positional naming one of `values` selects the operation. */
  operation?: { field: 'operation'; values: string[] };
  positionals: CliPositional[];
  flags: CliFlag[];
  presets: CliPreset[];
  splits: CliSplit[];
  examples: string[];
}

/** Fields every tool shares that a flag would only clutter: request notes stay JSON-only. */
const JSON_ONLY = new Set(['mainGoal', 'reasoning']);

function resolve(schema: unknown, defs: Json): Json {
  let node = (schema ?? {}) as Json;
  for (let depth = 0; depth < 16; depth++) {
    const ref = node.$ref;
    if (typeof ref === 'string' && ref.startsWith('#/$defs/')) {
      const { $ref: _ref, ...rest } = node;
      node = { ...(defs[ref.slice('#/$defs/'.length)] as Json), ...rest };
      continue;
    }
    if (Array.isArray(node.allOf) && node.allOf.length === 1) {
      const { allOf, ...rest } = node;
      node = { ...rest, ...(resolve(allOf[0], defs)) };
      continue;
    }
    return node;
  }
  return node;
}

function kindOf(raw: unknown, defs: Json): { kind: FlagKind; values?: string[] } {
  const schema = resolve(raw, defs);
  const variants = (schema.anyOf ?? schema.oneOf) as unknown[] | undefined;
  if (variants) {
    const kinds = variants.map(variant => kindOf(variant, defs));
    // `"a"` or `["a","b"]`: one repeatable text flag.
    if (kinds.some(k => k.kind === 'strings') && kinds.every(k => k.kind === 'strings' || k.kind === 'string'))
      return { kind: 'strings' };
    if (kinds.every(k => k.kind === 'string' || k.kind === 'enum')) return { kind: 'string' };
    return { kind: 'json' };
  }
  if (Array.isArray(schema.type)) return { kind: 'json' };
  if (Array.isArray(schema.enum)) return { kind: 'enum', values: schema.enum.map(String) };
  if (typeof schema.const === 'string') return { kind: 'enum', values: [schema.const] };
  switch (schema.type) {
    case 'boolean':
      return { kind: 'bool' };
    case 'integer':
      return { kind: 'integer' };
    case 'number':
      return { kind: 'number' };
    case 'string':
      return { kind: 'string' };
    case 'array': {
      const item = kindOf(schema.items, defs);
      if (item.kind === 'string') return { kind: 'strings' };
      if (item.kind === 'enum') return { kind: 'strings', values: item.values };
      if (item.kind === 'integer') return { kind: 'integers' };
      return { kind: 'json' };
    }
    default:
      return { kind: 'json' };
  }
}

interface SchemaField {
  /** The field's schema in each form that has it. */
  raws: unknown[];
  description: string;
  /** Each distinct description and the operations of the forms that give it. */
  described: Map<string, string[]>;
  /** The operations of the forms that accept it (empty: a form has none). */
  operations: Set<string>;
}

/** The operation names one form accepts: its `operation` const or enum. */
function formOperations(form: Json, defs: Json): string[] {
  const raw = (form.properties as Json | undefined)?.operation;
  if (!raw) return [];
  const operation = resolve(raw, defs);
  if (typeof operation.const === 'string') return [operation.const];
  if (Array.isArray(operation.enum)) return operation.enum.map(String);
  if (typeof operation.default === 'string') return [operation.default];
  return [];
}

/** Every field of a canonical row schema, merged across its forms. */
function schemaFields(querySchema: Json): Map<string, SchemaField> {
  const defs = (querySchema.$defs ?? {}) as Json;
  const forms = ((querySchema.anyOf ?? querySchema.oneOf) as Json[] | undefined) ?? [querySchema];
  const fields = new Map<string, SchemaField>();
  for (const form of forms) {
    const resolved = resolve(form, defs);
    const properties = (resolved.properties ?? {}) as Json;
    const operations = formOperations(resolved, defs);
    for (const [name, raw] of Object.entries(properties)) {
      const description = String(resolve(raw, defs).description ?? '');
      const forbidden = resolve(raw, defs).not !== undefined;
      const seen = fields.get(name) ?? {
        raws: [],
        description: '',
        described: new Map<string, string[]>(),
        operations: new Set<string>(),
      };
      fields.set(name, seen);
      seen.raws.push(raw);
      if (!seen.description && description) seen.description = description;
      if (description) seen.described.set(description, [...(seen.described.get(description) ?? []), ...operations]);
      if (!forbidden) for (const operation of operations) seen.operations.add(operation);
    }
  }
  // Forms that describe a field differently each name their operations.
  for (const field of fields.values())
    if (field.described.size > 1 && [...field.described.values()].every(ops => ops.length > 0))
      field.description = [...field.described]
        .map(([description, ops]) => `${[...new Set(ops)].join(', ')}: ${description}`)
        .join(' ');
  return fields;
}

/** A field's flag kind across its forms: enum values are the union of every form's. */
function fieldKind(field: SchemaField, defs: Json): { kind: FlagKind; values?: string[] } {
  // A form that forbids the field (`{"not":{}}`) says nothing about its type.
  const allowed = field.raws.filter(raw => resolve(raw, defs).not === undefined);
  const kinds = (allowed.length > 0 ? allowed : field.raws).map(raw => kindOf(raw, defs));
  const first = kinds[0]!;
  if (kinds.every(k => k.kind === first.kind && (k.values !== undefined) === (first.values !== undefined))) {
    if (!first.values) return first;
    return { kind: first.kind, values: [...new Set(kinds.flatMap(k => k.values ?? []))] };
  }
  return { kind: 'json' };
}

/** The field names a row schema accepts, across its forms. */
export function schemaFieldNames(querySchema: Json): Set<string> {
  return new Set(schemaFields(querySchema).keys());
}

const RESERVED_SHORTS = new Set(['h', 'V']);

/**
 * Free-text lists whose items never hold a comma (names, kinds, extensions,
 * field names, topics, line ranges), so `a,b` is two items. Globs, patterns
 * and search keywords keep one value per flag.
 */
const COMMA_LISTS = new Set(['symbolName', 'kinds', 'extensions', 'fields', 'topics', 'ranges']);

/** The CLI flag spec of one tool, checked against its canonical query schema. */
export function cliSpec(name: string, querySchema: Json, primaryFields: ReadonlySet<string>): CliSpec {
  const authored = AUTHORED[name];
  if (!authored) throw new Error(`cli-flags: no CLI spec for tool ${name}`);
  const defs = (querySchema.$defs ?? {}) as Json;
  const fields = schemaFields(querySchema);
  const fail = (message: string): never => {
    throw new Error(`cli-flags ${name}: ${message}`);
  };
  const operationField = fields.get('operation');
  const operation = operationField
    ? { field: 'operation' as const, values: fieldKind(operationField, defs).values ?? [] }
    : undefined;

  const shortOf = new Map<string, string>();
  const presets: CliPreset[] = [];
  const used = new Set<string>(RESERVED_SHORTS);
  const claim = (letter: string) => {
    if (letter.length !== 1) fail(`short '${letter}' is not one character`);
    if (used.has(letter)) fail(`short -${letter} is used twice`);
    used.add(letter);
  };
  for (const [letter, target] of Object.entries(authored.shorts ?? {})) {
    claim(letter);
    const [field, value] = target.split('=') as [string, string | undefined];
    const entry = fields.get(field) ?? fail(`-${letter} names unknown field '${field}'`);
    if (value === undefined) {
      shortOf.set(field, letter);
      continue;
    }
    const kind = fieldKind(entry, defs);
    if (!kind.values?.includes(value)) fail(`-${letter} sets ${field} to '${value}', which it does not accept`);
    presets.push({ short: letter, field, value, description: `${field}: "${value}"` });
  }
  const splits: CliSplit[] = [];
  if (authored.ownerRepo) {
    if (!fields.has('owner') || !fields.has('repo')) fail('-R needs owner and repo fields');
    claim('R');
    splits.push({
      short: 'R',
      fields: ['owner', 'repo'],
      separator: '/',
      valueName: 'OWNER/REPO',
      description: 'owner and repo, as gh -R',
    });
  }
  for (const positional of authored.positionals) {
    const named = typeof positional.field === 'string'
      ? [positional.field]
      : Array.isArray(positional.field)
        ? positional.field
        : Object.values(positional.field).flat();
    for (const field of [...named, ...(positional.lines ? [positional.lines.field] : [])])
      if (!fields.has(field)) fail(`positional ${positional.name} names unknown field '${field}'`);
    if (positional.omitWhen && (!fields.has(positional.omitWhen.field) || !operation?.values.includes(positional.omitWhen.operation)))
      fail(`positional ${positional.name} omitWhen names an unknown field or operation`);
    if (!Array.isArray(positional.field) && typeof positional.field === 'object')
      for (const op of Object.keys(positional.field))
        if (!operation?.values.includes(op)) fail(`positional ${positional.name} names unknown operation '${op}'`);
  }

  const flags: CliFlag[] = [...fields]
    .filter(([field]) => !JSON_ONLY.has(field))
    .map(([field, entry]) => {
      const base = entry.description || CLI_FIELD_FALLBACKS[field] || '';
      const noted = CLI_FIELD_NOTES[field] ? `${base} ${CLI_FIELD_NOTES[field]}` : base;
      // A field some operations reject says which take it, so a call that
      // would be refused is not suggested by the help.
      const all = operation?.values ?? [];
      const only = [...entry.operations].filter(op => all.includes(op));
      const left = all.filter(op => !only.includes(op));
      const scope =
        only.length > left.length ? `not ${left.join(', ')}` : `${only.join(', ')} only`;
      const description =
        field !== 'operation' && all.length > 1 && only.length > 0 && left.length > 0
          ? `${noted} (${scope})`.trim()
          : noted;
      const kind = fieldKind(entry, defs);
      return {
        field,
        kind: kind.kind,
        ...(kind.values && { values: kind.values }),
        primary: primaryFields.has(field),
        ...(kind.kind === 'strings' && COMMA_LISTS.has(field) && { commas: true as const }),
        description,
        ...(shortOf.has(field) && { short: shortOf.get(field) }),
      };
    });
  return {
    ...(operation && { operation }),
    positionals: authored.positionals,
    flags,
    presets,
    splits,
    examples: authored.examples,
  };
}

/** How to read a tool's text output, when its help says so. */
export function cliOutput(name: string): string | undefined {
  return CLI_TOOL_OUTPUT[name];
}

/** The flag-form examples of a tool's help. */
export function cliExamples(name: string): string[] {
  return AUTHORED[name]?.examples ?? [];
}
