/**
 * Zod's JSON Schema output in the catalog's dialect. `z.toJSONSchema` is the generator; this module only removes what the
 * catalog's finite vocabulary (see runtime/validation.ts) does not use and fixes one key order, so the committed file is stable.
 */
import type { z } from 'zod';

type Schema = Record<string, unknown>;

const KEY_ORDER = [
  'type', 'properties', 'additionalProperties', 'required', 'minLength', 'maxLength', 'pattern', 'format', 'minimum', 'exclusiveMinimum', 'maximum',
  'minItems', 'maxItems', 'uniqueItems', 'items', 'enum', 'const', 'default', 'oneOf', 'anyOf', 'allOf', 'not', 'if', 'then', 'else',
  'dependentRequired', 'minProperties', 'maxProperties', '$ref', 'description',
];
/** Subschema keywords that hold one schema, and those that hold a list of them. */
const SUBSCHEMAS = ['items', 'additionalProperties', 'not', 'if', 'then', 'else', 'anyOf', 'oneOf', 'allOf'] as const;

const isSchema = (value: unknown): value is Schema => typeof value === 'object' && value !== null && !Array.isArray(value);
const rank = (key: string): number => { const index = KEY_ORDER.indexOf(key); return index < 0 ? KEY_ORDER.length : index; };
const ordered = (node: Schema): Schema => Object.fromEntries(Object.entries(node).toSorted(([a], [b]) => rank(a) - rank(b)));
const isEmpty = (value: unknown): boolean => isSchema(value) && Object.keys(value).length === 0;

/** `anyOf` of only `type` members, or of one typed member and `null`, is one `type` array. */
function mergeTypes(members: readonly unknown[]): Schema | undefined {
  const typed = members.filter((member): member is Schema & { type: string } => isSchema(member) && typeof member.type === 'string');
  if (typed.length !== members.length) return undefined;
  if (typed.every(member => Object.keys(member).length === 1)) return { type: typed.map(member => member.type) };
  const [first, second] = typed;
  if (typed.length === 2 && first && second && Object.keys(second).length === 1 && second.type === 'null') return { ...first, type: [first.type, 'null'] };
  return undefined;
}

interface OverrideContext { readonly zodSchema: z.core.$ZodType; readonly jsonSchema: Schema }

/**
 * `z.toJSONSchema` `override`: `z.int()` states the safe-integer range as bounds, which the catalog states only when the source sets
 * one (an explicit `.min()`, `.gt()` or `.max()` check). Wrappers such as `.optional()` reach here too and carry no checks of their own.
 */
export function dropImplicitBounds({ zodSchema, jsonSchema }: OverrideContext): void {
  // oxlint-disable-next-line no-underscore-dangle -- `_zod` is Zod's documented internals entry point for tools that read a schema
  const def = zodSchema._zod.def;
  if (def.type !== 'number') return;
  const checks = 'checks' in def && Array.isArray(def.checks) ? def.checks as readonly z.core.$ZodCheck[] : [];
  // oxlint-disable-next-line no-underscore-dangle -- see above
  const sets = (name: string): boolean => checks.some(check => check._zod.def.check === name);
  if (jsonSchema.minimum === Number.MIN_SAFE_INTEGER && !sets('greater_than')) delete jsonSchema.minimum;
  if (jsonSchema.maximum === Number.MAX_SAFE_INTEGER && !sets('less_than')) delete jsonSchema.maximum;
}

/** Zod's spelling of what the catalog leaves implicit: the draft keywords around the root, string property names, unbounded extra properties, a constant's type. */
function withoutDefaults(value: Schema): Schema {
  const node = { ...value };
  delete node.$schema;
  delete node.$id;
  if (isSchema(node.propertyNames) && Object.keys(node.propertyNames).length === 1 && node.propertyNames.type === 'string') delete node.propertyNames;
  if (isEmpty(node.additionalProperties)) node.additionalProperties = true;
  if ('const' in node) delete node.type;
  return node;
}

const child = (value: unknown): unknown => Array.isArray(value) ? value.map(child) : isSchema(value) ? catalogSchema(value) : value;

export function catalogSchema(value: Schema): Schema {
  const node = withoutDefaults(value);
  if (isSchema(node.properties)) node.properties = Object.fromEntries(Object.entries(node.properties).map(([name, property]) => [name, child(property)]));
  for (const key of SUBSCHEMAS) if (key in node) node[key] = child(node[key]);
  const merged = Array.isArray(node.anyOf) ? mergeTypes(node.anyOf) : undefined;
  if (merged) { delete node.anyOf; Object.assign(node, merged); }
  return ordered(node);
}
