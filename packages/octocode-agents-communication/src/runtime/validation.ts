/**
 * Validator for the finite JSON Schema vocabulary used by the command catalog.
 *
 * No schema is authored here. Unsupported validation keywords fail closed during catalog loading, so a contract
 * extension cannot silently skip a new constraint. Error messages contain locations and rules, never rejected
 * payloads. String lengths count code points; integers are whole numbers (JSON cannot tell 5.0 from 5);
 * patterns are JavaScript regular expressions compiled with the `u` flag.
 */
import { ValidationError } from './errors.js';
import { catalogWhitespace, codePointLength, truncateCodePoints } from './text.js';

export type Json = null | boolean | number | string | Json[] | { [key: string]: Json };
export type JsonObject = { [key: string]: Json };
/** A catalog schema: a boolean or an object of the supported keywords. */
export type Schema = boolean | JsonObject;

const KEYWORDS: ReadonlySet<string> = new Set([
  'type', 'properties', 'required', 'additionalProperties', 'enum', 'const', 'minimum', 'maximum', 'exclusiveMinimum',
  'exclusiveMaximum', 'minLength', 'maxLength', 'pattern', 'minItems', 'maxItems', 'uniqueItems', 'items',
  'minProperties', 'maxProperties', 'oneOf', 'anyOf', 'allOf', 'not', 'if', 'then', 'else', 'dependentRequired',
  'description', 'default', 'format', '$schema', 'title',
]);

const has = (object: object, key: string): boolean => Object.hasOwn(object, key);
/** A JSON object: not null, not an array. */
export const isObject = (value: unknown): value is JsonObject => typeof value === 'object' && value !== null && !Array.isArray(value);
/** `value` when it is a JSON object, else an empty one: a vendor reply field that may be absent. */
export const objectOrEmpty = (value: unknown): JsonObject => (isObject(value) ? value : {});
/** `value` as a JSON object, else a contract error naming `what`. */
export function asObject(value: unknown, what: string): JsonObject {
  if (!isObject(value)) throw new ValidationError(`${what} must be a JSON object`);
  return value;
}

/** `value` as a list of JSON objects, else a contract error naming `what`. */
export function asObjects(value: unknown, what: string): JsonObject[] {
  if (!Array.isArray(value)) throw new ValidationError(`${what} must be a list`);
  return value.map((item, index) => asObject(item, `${what}[${index}]`));
}

/** The JSON object a stored column holds. */
export function parseObject(text: string, what: string): JsonObject {
  return asObject(JSON.parse(text), what);
}

/** The JSON objects a stored column holds. */
export function parseObjects(text: string, what: string): JsonObject[] {
  return asObjects(JSON.parse(text), what);
}

const isArray = (value: unknown): value is unknown[] => Array.isArray(value);
const asArray = (value: unknown): unknown[] => (isArray(value) ? value : []);

const patterns = new Map<string, RegExp>();

/** The compiled form of a catalog pattern; a pattern JavaScript cannot compile with `u` is a contract error. */
export function compilePattern(pattern: string): RegExp {
  let compiled = patterns.get(pattern);
  if (!compiled) {
    try {
      compiled = new RegExp(catalogWhitespace(pattern), 'u');
    } catch {
      throw new ValidationError('Unsupported catalog pattern: ' + pattern);
    }
    patterns.set(pattern, compiled);
  }
  return compiled;
}

export function checkSchema(schema: unknown): void {
  if (typeof schema === 'boolean') return;
  if (!isObject(schema)) throw new ValidationError('Invalid catalog schema');
  const unknown = Object.keys(schema).filter(key => !KEYWORDS.has(key));
  if (unknown.length) throw new ValidationError('Unsupported catalog schema keyword: ' + unknown.sort().join(', '));
  for (const name of Array.isArray(schema.type) ? schema.type : schema.type === undefined ? [] : [schema.type]) if (typeof name !== 'string' || !has(TYPES, name)) throw new ValidationError('Unsupported catalog schema type: ' + String(name));
  const properties = schema.properties;
  if (isObject(properties)) for (const sub of Object.values(properties)) checkSchema(sub);
  for (const key of ['items', 'additionalProperties', 'not', 'if', 'then', 'else']) if (has(schema, key)) checkSchema(schema[key]!);
  for (const key of ['oneOf', 'anyOf', 'allOf']) for (const sub of asArray(schema[key])) checkSchema(sub);
  if (typeof schema.pattern === 'string') compilePattern(schema.pattern);
}

/** UUID acceptance: 32 hex digits once braces, hyphens and a `urn:uuid:` prefix are removed. */
export function isUuid(value: string): boolean {
  return /^[0-9a-f]{32}$/i.test(value.replaceAll('urn:', '').replaceAll('uuid:', '').replace(/^[{}]+|[{}]+$/g, '').replaceAll('-', ''));
}

/** JSON equality in which a boolean never equals a number. */
export function jsonEqual(a: unknown, b: unknown): boolean {
  if (isArray(a) && isArray(b)) return a.length === b.length && a.every((item, index) => jsonEqual(item, b[index]));
  if (isObject(a) && isObject(b)) {
    const keys = Object.keys(a);
    return keys.length === Object.keys(b).length && keys.every(key => has(b, key) && jsonEqual(a[key], b[key]));
  }
  return a === b;
}

const TYPES: Record<string, (value: unknown) => boolean> = {
  object: isObject,
  array: isArray,
  string: value => typeof value === 'string',
  boolean: value => typeof value === 'boolean',
  null: value => value === null,
  number: value => typeof value === 'number' && Number.isFinite(value),
  integer: value => typeof value === 'number' && Number.isInteger(value),
};

function number(schema: JsonObject, key: string, fallback: number): number {
  const value = schema[key];
  return typeof value === 'number' ? value : fallback;
}

export function validate(schema: Schema, value: unknown, location = ''): void {
  const fail = (rule: string): never => {
    throw new ValidationError(truncateCodePoints('Invalid input: ' + (location || 'input') + ' ' + rule, 200));
  };
  if (schema === true) return;
  if (schema === false) return fail('is not allowed');
  const kind = schema.type;
  if (kind && !(Array.isArray(kind) ? kind : [kind]).some(name => TYPES[name as string]!(value))) fail('has invalid type');
  if (has(schema, 'enum') && !asArray(schema.enum).some(item => jsonEqual(value, item))) fail('is not an allowed value');
  if (has(schema, 'const') && !jsonEqual(value, schema.const)) fail('does not match required value');
  if (isObject(value)) {
    const missingKey = asArray(schema.required).find(key => !has(value, key as string));
    if (missingKey !== undefined) fail('is missing required property ' + missingKey);
    const count = Object.keys(value).length;
    if (count < number(schema, 'minProperties', 0) || count > number(schema, 'maxProperties', Infinity)) fail('has invalid property count');
    const properties = isObject(schema.properties) ? schema.properties : {};
    const additional = schema.additionalProperties;
    for (const [key, item] of Object.entries(value)) {
      const child = location + '/' + key.replaceAll('~', '~0').replaceAll('/', '~1');
      if (has(properties, key)) validate(properties[key] as Schema, item, child);
      else if (additional === false) fail('has additional properties');
      else if (isObject(additional)) validate(additional, item, child);
    }
    const dependent = schema.dependentRequired;
    if (isObject(dependent))
      for (const [key, needed] of Object.entries(dependent)) if (has(value, key) && asArray(needed).some(other => !has(value, other as string))) fail('is missing a dependent property');
  }
  if (isArray(value)) {
    if (value.length < number(schema, 'minItems', 0) || value.length > number(schema, 'maxItems', Infinity)) fail('has invalid item count');
    if (schema.uniqueItems && value.some((item, index) => value.slice(0, index).some(previous => jsonEqual(item, previous)))) fail('must contain unique items');
    if (has(schema, 'items')) value.forEach((item, index) => validate(schema.items as Schema, item, location + '/' + index));
  }
  if (typeof value === 'string') {
    const length = codePointLength(value);
    if (length < number(schema, 'minLength', 0) || length > number(schema, 'maxLength', Infinity)) fail('has invalid string length');
    if (typeof schema.pattern === 'string' && !compilePattern(schema.pattern).test(value)) fail('does not match pattern');
  }
  if (typeof value === 'number') {
    if (value < number(schema, 'minimum', -Infinity) || value > number(schema, 'maximum', Infinity)) fail('is outside numeric bounds');
    if (value <= number(schema, 'exclusiveMinimum', -Infinity) || value >= number(schema, 'exclusiveMaximum', Infinity)) fail('is outside exclusive numeric bounds');
  }
  const matches = (sub: unknown): boolean => {
    try {
      validate(sub as Schema, value, location);
      return true;
    } catch (error) {
      if (error instanceof ValidationError) return false;
      throw error;
    }
  };
  if (has(schema, 'allOf') && !asArray(schema.allOf).every(matches)) fail('does not satisfy allOf');
  if (has(schema, 'anyOf') && !asArray(schema.anyOf).some(matches)) fail('does not satisfy anyOf');
  if (has(schema, 'oneOf') && asArray(schema.oneOf).filter(matches).length !== 1) fail('must match exactly one alternative');
  if (has(schema, 'not') && matches(schema.not!)) fail('matches a forbidden alternative');
  if (has(schema, 'if')) {
    const branch = matches(schema.if!) ? 'then' : 'else';
    if (has(schema, branch)) validate(schema[branch] as Schema, value, location);
  }
}

/** True when every string and object key in `value` is well-formed UTF-16 (no lone surrogate): JSON text and SQLite need valid UTF-8. */
export function wellFormed(value: unknown): boolean {
  if (typeof value === 'string') return value.isWellFormed();
  if (Array.isArray(value)) return value.every(wellFormed);
  if (typeof value === 'object' && value !== null) return Object.entries(value).every(([key, item]) => key.isWellFormed() && wellFormed(item));
  return true;
}
