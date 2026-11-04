// Evidence query predicates: exact JSON reads (large integers stay raw), JSON pointers, and the
// where/select inputs that filter and project rows before pagination.
import { compareNumeric } from './evidence-numeric.mjs';

/** One --where predicate: { path, op: eq|ne|contains|gte|lte|exists, value }. */
export type Predicate = { path: string; op: string; value: unknown };
/** A number JSON.rawJSON keeps as its exact source text. */
type RawJson = { rawJSON: string };

const OPS = ['eq', 'ne', 'contains', 'gte', 'lte', 'exists'];
const isRawJson = (value: unknown): value is RawJson => JSON.isRawJSON(value);
const INTEGER = /^-?\d+$/;

/** JSON.parse that keeps every number as its exact source text. */
export const parse = (text: string): unknown =>
  JSON.parse(text, (_key, item, context) =>
    typeof item === 'number' && context?.source
      ? JSON.rawJSON(context.source)
      : item
  );

/**
 * A JSON pointer for a path written either way: `/response/status` stays, `response.status` and
 * `entries.0.request` become pointers. A key that contains a dot needs the pointer form.
 */
export function toPointer(path: string): string {
  if (!path || path.startsWith('/')) return path;
  return (
    '/' +
    path
      .split('.')
      .map(part => part.replace(/~/g, '~0').replace(/\//g, '~1'))
      .join('/')
  );
}

const WHERE_EXAMPLE =
  'response.status=0 && request.url~login, or JSON [{"path":"/response/status","op":"eq","value":0}]';

/** A comparison's value: a JSON literal (number, true, false, null, "text") when it parses, else the text. */
function literal(text: string): unknown {
  try {
    return parse(text);
  } catch {
    return text;
  }
}

function simplePredicate(clause: string): Predicate {
  const text = clause.trim();
  const match = /^(.+?)(!=|>=|<=|=|~)(.*)$/.exec(text);
  if (match) {
    const [, path, symbol, value] = match;
    const op = {
      '=': 'eq',
      '!=': 'ne',
      '>=': 'gte',
      '<=': 'lte',
      '~': 'contains',
    }[symbol]!;
    return {
      path: toPointer(path.trim()),
      op,
      value: op === 'contains' ? value.trim() : literal(value.trim()),
    };
  }
  if (/^[^\s=<>!~]+$/.test(text))
    return { path: toPointer(text), op: 'exists', value: true };
  if (/^![^\s=<>!~]+$/.test(text))
    return { path: toPointer(text.slice(1)), op: 'exists', value: false };
  throw new Error(`Cannot read the condition "${clause.trim()}"`);
}

/**
 * The --where text as predicates: JSON (an array, or one object) or simple conditions joined by &&:
 * `path=value`, `path!=value`, `path>=n`, `path<=n`, `path~text`, `path` (exists), `!path` (missing).
 */
export function parseWhere(text: string): unknown {
  const trimmed = text.trim();
  try {
    if (trimmed.startsWith('[')) return parse(trimmed);
    if (trimmed.startsWith('{')) return [parse(trimmed)];
    return trimmed.split('&&').map(simplePredicate);
  } catch (error) {
    throw new Error(
      `Invalid --where: ${error instanceof Error ? error.message : String(error)}. Example: ${WHERE_EXAMPLE}`,
      { cause: error }
    );
  }
}

/** The --select text as pointers: a JSON array, or comma-separated paths (`request.url,/response/status`). */
export function parseSelect(text: string): string[] {
  const trimmed = text.trim();
  let paths: unknown;
  try {
    paths = trimmed.startsWith('[')
      ? JSON.parse(trimmed)
      : trimmed.split(',').map(part => part.trim());
  } catch (error) {
    throw new Error(
      `Invalid --select: ${error instanceof Error ? error.message : String(error)}. Example: request.url,response.status or ["/request/url"]`,
      { cause: error }
    );
  }
  if (!Array.isArray(paths) || paths.some(p => typeof p !== 'string' || !p))
    throw new Error(
      'Invalid --select: need paths. Example: request.url,response.status or ["/request/url"]'
    );
  return paths.map(toPointer);
}

/** The value at a JSON pointer, or undefined when a key is missing; an invalid pointer fails. */
export function pointer(value: unknown, path: string): unknown {
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
    current = Reflect.get(current, key);
  }
  return current;
}

const beyondSafeInteger = (actual: unknown) =>
  isRawJson(actual) &&
  INTEGER.test(actual.rawJSON) &&
  (BigInt(actual.rawJSON) > BigInt(Number.MAX_SAFE_INTEGER) ||
    BigInt(actual.rawJSON) < BigInt(Number.MIN_SAFE_INTEGER));

// Integer strings are the existing exact-large-integer query form.
function expectedValue(filter: Predicate, actual: unknown) {
  const expected = filter.value;
  if (
    typeof expected === 'string' &&
    INTEGER.test(expected) &&
    (['gte', 'lte'].includes(filter.op) || beyondSafeInteger(actual))
  )
    return BigInt(expected);
  return expected;
}

function matchesFilter(row: unknown, filter: Predicate): boolean {
  if (filter.op === 'ne') return !matchesFilter(row, { ...filter, op: 'eq' });
  const actual = pointer(row, filter.path);
  if (filter.op === 'exists') return (actual !== undefined) === filter.value;
  if (filter.op === 'contains')
    return (
      typeof actual === 'string' &&
      typeof filter.value === 'string' && // validatePredicate: contains needs text
      actual.includes(filter.value)
    );
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
export const matches = (row: unknown, filters: Predicate[]) =>
  filters.every(filter => matchesFilter(row, filter));

const isScalar = (value: unknown) =>
  value === null ||
  JSON.isRawJSON(value) ||
  ['string', 'number', 'boolean'].includes(typeof value);

const isNumericValue = (value: unknown) =>
  typeof value === 'number' ||
  JSON.isRawJSON(value) ||
  INTEGER.test(String(value));

function validatePredicate(f: Predicate) {
  if (Object.keys(f).some(key => !['path', 'op', 'value'].includes(key)))
    throw new Error('Unknown predicate field');
  if (['eq', 'ne'].includes(f.op) && !isScalar(f.value))
    throw new Error(`${f.op} needs a scalar value`);
  f.path = toPointer(f.path);
  pointer({}, f.path);
  if (f.op === 'exists' && typeof f.value !== 'boolean')
    throw new Error('exists needs a boolean');
  if (f.op === 'contains' && typeof f.value !== 'string')
    throw new Error('contains needs text');
  if (['gte', 'lte'].includes(f.op) && !isNumericValue(f.value))
    throw new Error('Numeric predicate needs a number or integer string');
}

/** Checks --where predicates: [{ path, op: eq|ne|contains|gte|lte|exists, value }]; dotted paths become pointers. */
export function validatePredicates(
  filters: unknown
): asserts filters is Predicate[] {
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

/** Checks --select: a nonempty array of JSON pointers; returns them, or null for no projection. */
export function validateProjection(select: unknown): string[] | null {
  if (!select) return null;
  if (
    !Array.isArray(select) ||
    !select.length ||
    select.some(p => typeof p !== 'string')
  )
    throw new Error('Invalid projection');
  for (const p of select) pointer({}, p);
  return select;
}
