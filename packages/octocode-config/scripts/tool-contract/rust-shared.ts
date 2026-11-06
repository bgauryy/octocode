import { createHash } from 'node:crypto';
import { isObject, type Json, type JsonObject } from './json.ts';

const schemaMaps = new Set(['$defs', 'definitions', 'properties', 'patternProperties', 'dependentSchemas']);
const schemaArrays = new Set(['allOf', 'anyOf', 'oneOf', 'prefixItems']);
const schemaValues = new Set([
  'additionalProperties', 'unevaluatedProperties', 'propertyNames', 'items',
  'additionalItems', 'unevaluatedItems', 'contains', 'not', 'if', 'then', 'else',
]);

/** Visit schemas, not JSON values in const/default/enum/examples or property names. */
function walk(schema: Json, visit: (schema: JsonObject, named: boolean) => Json, named = false): Json {
  if (!isObject(schema)) return schema;
  const result = Object.fromEntries(Object.entries(schema).map(([key, value]) => {
    if (schemaMaps.has(key) && isObject(value))
      return [key, Object.fromEntries(Object.entries(value).map(([name, child]) =>
        [name, walk(child, visit, key === '$defs' || key === 'definitions')]))];
    if (schemaArrays.has(key) && Array.isArray(value))
      return [key, value.map(child => walk(child, visit))];
    if (schemaValues.has(key))
      return [key, Array.isArray(value) ? value.map(child => walk(child, visit)) : walk(value, visit)];
    return [key, value];
  }));
  return visit(result, named);
}

const canonical = (value: Json): Json => Array.isArray(value)
  ? value.map(canonical)
  : isObject(value)
    ? Object.fromEntries(Object.entries(value)
      .sort(([left], [right]) => left < right ? -1 : left > right ? 1 : 0)
      .map(([key, child]) => [key, canonical(child)]))
    : value;

/**
 * A name read from the constraint itself, so it changes only when the constraint does:
 * `TextMin1Max128`; anything beyond the length bounds (pattern, allOf, extension keys)
 * adds 8 hex of the canonical shape's sha256, as in `TextMin1P820fd8a1`.
 */
function constraintName(shape: JsonObject, key: string): string {
  const lengths = `${typeof shape.minLength === 'number' ? `Min${shape.minLength}` : ''}${typeof shape.maxLength === 'number' ? `Max${shape.maxLength}` : ''}`;
  const lengthOnly = Object.keys(shape).every(name => name === 'type' || name === 'minLength' || name === 'maxLength');
  return `Text${lengths}${lengthOnly ? '' : `P${createHash('sha256').update(key).digest('hex').slice(0, 8)}`}`;
}

/** Reuse typify's validated newtypes through JSON Schema refs instead of compiling one per field. */
export function shareStringSchemas(bundle: JsonObject): JsonObject {
  const shapes = new Map<string, { shape: JsonObject; uses: number }>();
  const parts = (schema: JsonObject, named: boolean) => {
    if (named || schema.title !== undefined || schema.$ref !== undefined || schema.type !== 'string'
      || !['minLength', 'maxLength', 'pattern', 'allOf'].some(key => schema[key] !== undefined)) return undefined;
    const { description, default: fallback, examples, ...shape } = schema;
    return { shape, key: JSON.stringify(canonical(shape)), annotations: {
      ...(description === undefined ? {} : { description }),
      ...(fallback === undefined ? {} : { default: fallback }),
      ...(examples === undefined ? {} : { examples }),
    } };
  };
  walk(bundle, (schema, named) => {
    const part = parts(schema, named);
    if (part) {
      const existing = shapes.get(part.key);
      if (existing) existing.uses++;
      else shapes.set(part.key, { shape: part.shape, uses: 1 });
    }
    return schema;
  });
  const definitions: JsonObject = {};
  const names = new Map<string, string>();
  const result = walk(bundle, (schema, named) => {
    const part = parts(schema, named);
    if (!part || (shapes.get(part.key)?.uses ?? 0) < 2) return schema;
    const name = constraintName(part.shape, part.key);
    const existing = isObject(bundle.$defs) ? bundle.$defs[name] : undefined;
    if (existing !== undefined) throw new Error(`Reserved generated string type: ${name}`);
    const claimed = names.get(name);
    if (claimed !== undefined && claimed !== part.key) throw new Error(`Generated string type ${name} names two constraint sets`);
    names.set(name, part.key);
    definitions[name] = part.shape;
    return { $ref: `#/$defs/${name}`, ...part.annotations };
  }) as JsonObject;
  return { ...result, $defs: { ...(isObject(result.$defs) ? result.$defs : {}), ...definitions } };
}
