// The outline help prints under a partial JSON flag: each field of the flag's schema with
// its type word, description, enum values, required mark and default, nested fields indented.
import { asSchema, deref, schemaList } from './json-schema.js';
import type { JsonSchema } from './spec.js';
import { shellWord } from './usage.js';

export function aligned(rows: Array<{ head: string; tail: string }>, indent: string): string[] {
  const width = Math.max(1, ...rows.map(row => row.head.length));
  return rows.map(row => `${indent}${row.head.padEnd(width)}   ${row.tail}`.trimEnd());
}

/** A non-object `const` as String() prints it; undefined for objects, null, and no const. */
function constText(value: unknown): string | undefined {
  switch (typeof value) {
    case 'string':
    case 'number':
    case 'boolean':
    case 'bigint':
    case 'symbol':
    case 'function':
      return String(value);
    default:
      return undefined;
  }
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
  const constant = constText(schema.const);
  if (constant !== undefined) parts.push(shellWord(constant));
  if (Array.isArray(schema.enum)) parts.push(`{${schema.enum.map(value => shellWord(String(value))).join('|')}}`);
  if (required) parts.push('(required)');
  if (Object.prototype.hasOwnProperty.call(schema, 'default')) parts.push(`(default ${JSON.stringify(schema.default)})`);
  return parts.join(' ');
}

function indentOf(depth: number): string {
  return ' '.repeat(10 + depth * 2);
}

interface OutlineContext { depth: number; ancestors: ReadonlySet<JsonSchema> }

export function outlineLines(schema: JsonSchema, root: JsonSchema, prefix: string,
  context: OutlineContext = { depth: 0, ancestors: new Set() }): string[] {
  const { depth, ancestors } = context;
  const resolved = deref(root, schema);
  if (ancestors.has(resolved)) return [`${indentOf(depth)}${prefix}(recursive)`];
  const seen = new Set(ancestors).add(resolved);
  const alternatives = [...schemaList(resolved.anyOf), ...schemaList(resolved.oneOf)];
  if (alternatives.length > 0) {
    return alternatives.flatMap((branch, index) => {
      const part = deref(root, branch);
      const merged = { ...resolved, ...part,
        properties: { ...asSchema(resolved.properties), ...asSchema(part.properties) },
        required: [...new Set([...(Array.isArray(resolved.required) ? resolved.required : []),
          ...(Array.isArray(part.required) ? part.required : [])])],
      };
      return [
        `${indentOf(depth)}${prefix}shape ${index + 1}`,
        ...fieldLines(merged, root, prefix, { depth, ancestors: seen }),
      ];
    });
  }
  return fieldLines(resolved, root, prefix, { depth, ancestors: seen });
}

function nestedLines(schema: JsonSchema, root: JsonSchema, depth: number, ancestors: ReadonlySet<JsonSchema>): string[] {
  if (schema.type === 'object' || asSchema(schema.properties) || schema.anyOf || schema.oneOf) {
    return outlineLines(schema, root, '', { depth: depth + 1, ancestors });
  }
  if (schema.type === 'array') return outlineLines(schema, root, '', { depth: depth + 1, ancestors });
  return [];
}

function fieldLines(schema: JsonSchema, root: JsonSchema, prefix: string, context: OutlineContext): string[] {
  const { depth, ancestors } = context;
  const resolved = deref(root, schema);
  if (resolved.type === 'array') {
    const items = asSchema(resolved.items);
    if (!items) return [];
    const item = deref(root, items);
    if (item.type === 'object' || asSchema(item.properties) || item.anyOf || item.oneOf) {
      return outlineLines(item, root, `${prefix}[ ].`, { depth, ancestors });
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
      children: nestedLines(resolvedChild, root, depth, ancestors),
    });
  }
  const rendered = aligned(rows.map(row => ({ head: row.head, tail: row.tail })), indentOf(depth));
  return rows.flatMap((row, index) => [rendered[index] ?? '', ...row.children]);
}
