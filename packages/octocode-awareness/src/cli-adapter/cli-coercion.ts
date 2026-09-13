import { AwarenessInputError } from '../command-output.js';

function variants(schema: Record<string, unknown>): Record<string, unknown>[] {
  return [schema, ...['anyOf', 'oneOf'].flatMap(key => Array.isArray(schema[key])
    ? (schema[key] as Record<string, unknown>[]).flatMap(variants) : [])];
}

/** Coerce only schema-declared shapes; strict validation remains with the domain. */
export function coerceFlag(value: unknown, schema: Record<string, unknown>): unknown {
  const choices = variants(schema);
  const array = choices.find(candidate => candidate.type === 'array');
  if (Array.isArray(value)) {
    if (array) return value.map(item => coerceFlag(item, array.items as Record<string, unknown> ?? {}));
    return value.length === 1 ? coerceFlag(value[0], schema) : value;
  }
  if (typeof value === 'string') {
    if (value === 'null' && choices.some(candidate => candidate.type === 'null')) return null;
    if (array && value.trim().startsWith('[')) {
      try {
        const parsed: unknown = JSON.parse(value);
        if (Array.isArray(parsed)) return parsed.map(item => coerceFlag(item, array.items as Record<string, unknown> ?? {}));
      } catch { throw new AwarenessInputError('Array flags require valid JSON'); }
    }
    if (choices.some(candidate => candidate.type === 'object')) {
      try { return JSON.parse(value) as unknown; }
      catch { throw new AwarenessInputError('Object flags require valid JSON'); }
    }
    if (choices.some(candidate => candidate.type === 'integer' || candidate.type === 'number') && value.trim()) {
      const number = Number(value);
      if (Number.isFinite(number)) return number;
    }
    if (choices.some(candidate => candidate.type === 'boolean')) {
      if (['true', 'yes', '1'].includes(value.toLowerCase())) return true;
      if (['false', 'no', '0'].includes(value.toLowerCase())) return false;
    }
  }
  if (array && !choices.some(candidate => candidate.type === typeof value)) {
    return [coerceFlag(value, array.items as Record<string, unknown> ?? {})];
  }
  return value;
}
