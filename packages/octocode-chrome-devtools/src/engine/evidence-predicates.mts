// Evidence query predicates: exact JSON reads (large integers stay raw), JSON pointers, and the
// where/select inputs that filter and project rows before pagination.
import { compareNumeric } from './evidence-numeric.mjs';

const OPS = ['eq', 'contains', 'gte', 'lte', 'exists'];
const INTEGER = /^-?\d+$/;

/** JSON.parse that keeps every number as its exact source text. */
export const parse = text =>
  JSON.parse(text, (_key, item, context) =>
    typeof item === 'number' && context?.source
      ? JSON.rawJSON(context.source)
      : item
  );

/** The value at a JSON pointer, or undefined when a key is missing; an invalid pointer fails. */
export function pointer(value, path) {
  if (!path) return value;
  if (!path.startsWith('/')) throw new Error('JSON pointer must start with /');
  let current = value;
  for (const part of path.slice(1).split('/')) {
    if (/~(?![01])/u.test(part)) throw new Error('Invalid JSON pointer escape');
    const key = part.replace(/~1/g, '/').replace(/~0/g, '~');
    if (
      current === null ||
      typeof current !== 'object' ||
      JSON.isRawJSON(current) ||
      !Object.hasOwn(current, key)
    )
      return undefined;
    current = current[key];
  }
  return current;
}

const beyondSafeInteger = actual =>
  JSON.isRawJSON(actual) &&
  INTEGER.test(actual.rawJSON) &&
  (BigInt(actual.rawJSON) > BigInt(Number.MAX_SAFE_INTEGER) ||
    BigInt(actual.rawJSON) < BigInt(Number.MIN_SAFE_INTEGER));

// Integer strings are the existing exact-large-integer query form.
function expectedValue(filter, actual) {
  const expected = filter.value;
  if (
    typeof expected === 'string' &&
    INTEGER.test(expected) &&
    (['gte', 'lte'].includes(filter.op) || beyondSafeInteger(actual))
  )
    return BigInt(expected);
  return expected;
}

function matchesFilter(row, filter) {
  const actual = pointer(row, filter.path);
  if (filter.op === 'exists') return (actual !== undefined) === filter.value;
  if (filter.op === 'contains')
    return typeof actual === 'string' && actual.includes(filter.value);
  const expected = expectedValue(filter, actual);
  const order = compareNumeric(actual, expected);
  if (filter.op === 'eq')
    return order === null ? actual === expected : order === 0;
  if (order === null) return false;
  if (filter.op === 'gte') return order >= 0;
  if (filter.op === 'lte') return order <= 0;
  return false;
}

/** Whether a row satisfies every predicate. */
export const matches = (row, filters) =>
  filters.every(filter => matchesFilter(row, filter));

const isScalar = value =>
  value === null ||
  JSON.isRawJSON(value) ||
  ['string', 'number', 'boolean'].includes(typeof value);

const isNumericValue = value =>
  typeof value === 'number' ||
  JSON.isRawJSON(value) ||
  INTEGER.test(String(value));

function validatePredicate(f) {
  if (Object.keys(f).some(key => !['path', 'op', 'value'].includes(key)))
    throw new Error('Unknown predicate field');
  if (f.op === 'eq' && !isScalar(f.value))
    throw new Error('eq needs a scalar value');
  pointer({}, f.path);
  if (f.op === 'exists' && typeof f.value !== 'boolean')
    throw new Error('exists needs a boolean');
  if (f.op === 'contains' && typeof f.value !== 'string')
    throw new Error('contains needs text');
  if (['gte', 'lte'].includes(f.op) && !isNumericValue(f.value))
    throw new Error('Numeric predicate needs a number or integer string');
}

/** Checks --where predicates: [{ path, op: eq|contains|gte|lte|exists, value }]. */
export function validatePredicates(filters) {
  if (
    !Array.isArray(filters) ||
    filters.some(
      f =>
        !f ||
        typeof f.path !== 'string' ||
        !OPS.includes(f.op) ||
        !Object.hasOwn(f, 'value')
    )
  )
    throw new Error('Invalid predicates');
  for (const f of filters) validatePredicate(f);
}

/** Checks --select: a nonempty array of JSON pointers. */
export function validateProjection(select) {
  if (
    select &&
    (!Array.isArray(select) ||
      !select.length ||
      select.some(p => typeof p !== 'string'))
  )
    throw new Error('Invalid projection');
  for (const p of select || []) pointer({}, p);
}
