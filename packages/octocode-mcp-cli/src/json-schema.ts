// Read-only navigation of a JSON Schema: narrow a value to a schema object, list a
// keyword's schemas, and follow local `$ref`s.
import type { JsonSchema } from './spec.js';

/** A plain JSON object (not an array). */
export function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === 'object' && !Array.isArray(value);
}

/** Any object a JSON pointer can step into (arrays included). */
function isIndexable(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === 'object';
}

export function asSchema(value: unknown): JsonSchema | undefined {
  return isRecord(value) ? value : undefined;
}

export function schemaList(value: unknown): JsonSchema[] {
  if (!Array.isArray(value)) return [];
  return value.map(asSchema).filter((item): item is JsonSchema => item !== undefined);
}

export function deref(root: JsonSchema, schema: JsonSchema, seen = new Set<string>()): JsonSchema {
  const ref = schema.$ref;
  if (typeof ref !== 'string' || !ref.startsWith('#/') || seen.has(ref)) return schema;
  seen.add(ref);
  let cursor: unknown = root;
  for (const part of ref.slice(2).split('/').map(decodeURIComponent)) {
    if (!isIndexable(cursor)) return schema;
    cursor = cursor[part];
  }
  const next = asSchema(cursor);
  return next ? deref(root, next, seen) : schema;
}
