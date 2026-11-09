// What a later part of a split row may repeat: per row pointer, the page context the contract
// declares, and the empty forms its output schema admits there.
import { isDeepStrictEqual } from 'node:util';
import { contract } from './contract.mjs';

// Per-row-pointer page context (`pageContextFields`, `collectionPaths` format, `*` per element), the one
// source every oracle reads: `required` repeats whole on every part, `requiredEvidence` arrives once (empty
// on later parts), `optional` repeats whole when present. A pointer in none of them rides the first part only.
const CONTEXT_KINDS = ['required', 'requiredEvidence', 'optional'];
const contexts = new Map();
function toolContext(name) {
  if (!contexts.has(name)) {
    const tool = contract.tools.find(entry => entry.name === name);
    if (!tool) throw new Error(`no contract for tool ${name}`);
    contexts.set(name, { tool, patterns: Object.fromEntries(CONTEXT_KINDS.map(kind => [kind, (tool.pageContextFields[kind] ?? []).map(pointer => pointer.split('/').slice(1))])) });
  }
  return contexts.get(name);
}
const matchesPattern = (pattern, segments) => pattern.length === segments.length && pattern.every((part, at) => part === '*' || part === segments[at]);

/** What a later part carries at `segments` of `tool`'s row: the nearest declared kind, ancestors included; undefined: first part only. */
function contextKind(tool, segments) {
  const { patterns } = toolContext(tool);
  for (let end = segments.length; end > 0; end--) {
    const here = segments.slice(0, end);
    const kinds = CONTEXT_KINDS.filter(kind => patterns[kind].some(pattern => matchesPattern(pattern, here)));
    if (kinds.includes('required')) return 'required';
    if (kinds.includes('optional')) return 'optional';
    if (kinds.includes('requiredEvidence')) return end === segments.length ? 'requiredEvidence' : undefined;
  }
  return undefined;
}

// Keywords that bound a value so the output schema may admit no empty form of it (native `output_fields.rs`
// VALUE_BOUNDS, LIST_BOUNDS, RECORD_BOUNDS); a minimum of zero bounds nothing.
const VALUE_BOUNDS = ['enum', 'const', 'format', 'pattern', 'minLength', 'maxLength'];
const LIST_BOUNDS = ['minItems', 'prefixItems', 'contains', 'minContains', 'enum', 'const'];
const RECORD_BOUNDS = ['minProperties', 'enum', 'const'];
function schemaBranches(schema, root, seen = new Set()) {
  if (!schema || typeof schema !== 'object' || seen.has(schema)) return [];
  seen.add(schema);
  const target = typeof schema.$ref === 'string' ? root.$defs?.[schema.$ref.replace(/^#\/\$defs\//, '')] : undefined;
  return [schema, ...(target ? schemaBranches(target, root, seen) : []), ...['anyOf', 'oneOf', 'allOf'].flatMap(key => (schema[key] ?? []).flatMap(branch => schemaBranches(branch, root, seen)))];
}
/** Whether `tool`'s output schema reaches `segments` (a row pointer) and bounds the value there with none of `bounds`. */
function unboundedAt(tool, segments, bounds) {
  const root = toolContext(tool).tool.outputSchema;
  // As native `schema_facts::child_schemas`: a named property, else typed record entries; an array index
  // reaches the items when no record entry matches.
  const step = (schemas, key) => {
    const branches = schemas.flatMap(schema => schemaBranches(schema, root));
    const children = branches.map(branch => branch.properties?.[key] ?? (isRecord(branch.additionalProperties) ? branch.additionalProperties : undefined)).filter(Boolean);
    return children.length > 0 || !/^\d+$/.test(key) ? children : branches.map(branch => branch.items).filter(Boolean);
  };
  let at = step(step([root], 'results'), '0');
  for (const key of segments) at = step(at, key);
  const branches = at.flatMap(schema => schemaBranches(schema, root));
  return branches.length > 0 && !branches.some(branch => bounds.some(bound => bound in branch && branch[bound] !== 0));
}

const isRecord = value => value !== null && typeof value === 'object' && !Array.isArray(value);
const executableCall = value => isRecord(value) && 'tool' in value && 'input' in value;

/** Whether the output schema admits the empty form of `original` at `segments`: free text `""`, an unbounded list `[]`, an unbounded record member by member. */
function emptiableAt(tool, segments, original) {
  if (typeof original === 'string') return unboundedAt(tool, segments, VALUE_BOUNDS);
  if (Array.isArray(original)) return unboundedAt(tool, segments, LIST_BOUNDS);
  if (isRecord(original)) return !executableCall(original) && unboundedAt(tool, segments, RECORD_BOUNDS);
  return false;
}

/**
 * Whether `value` is what a later part may carry at `segments` of `tool`'s row given the `original` first-part
 * value: identity (required, optional) repeated whole; required evidence in the empty form the output schema
 * admits there (free text `""`, a list `[]`, an object member by member), or whole and equal where the schema
 * admits none (a number, an enum, a bounded string, list or record, an executable call); anything undeclared
 * must be a nested object whose members are unchanged context. A repeated body or a changed identity is
 * evidence, never context.
 */
export function laterPartValue(tool, segments, value, original) {
  const kind = contextKind(tool, segments);
  const members = () => isRecord(original) && Object.entries(value).every(([name, child]) => Object.hasOwn(original, name) && laterPartValue(tool, [...segments, name], child, original[name]));
  if (kind === 'required' || kind === 'optional') return isDeepStrictEqual(value, original);
  if (kind === 'requiredEvidence') {
    if (!emptiableAt(tool, segments, original)) return isDeepStrictEqual(value, original);
    if (typeof value === 'string') return value === '' && typeof original === 'string';
    if (Array.isArray(value)) return value.length === 0 && Array.isArray(original);
    return isRecord(value) && members();
  }
  return isRecord(value) && members();
}

/**
 * Whether `row` at `segments` of `tool` is only the context a later part repeats of `original`: every
 * required identity of the original repeats, and no field (other than those `skip` names: collections and
 * evidence judged separately) carries new, changed or repeated evidence.
 */
export function laterPartContext(tool, segments, { row, original, skip = () => false }) {
  if (!isRecord(row) || !isRecord(original)) return false;
  return Object.keys(original).filter(key => contextKind(tool, [...segments, key]) === 'required').every(key => Object.hasOwn(row, key))
    && Object.entries(row).every(([key, value]) => skip(key) || (Object.hasOwn(original, key) && laterPartValue(tool, [...segments, key], value, original[key])));
}

/**
 * The values of identity `fields` for one listed row: its own, else the page's `data`, else the
 * query that asked. A fresh response states a repository shared by every row once on `data`, or
 * leaves it to the query when the page's value equals it; a cursor page keeps it on `data`. All
 * three name the same repository, so row keys resolve it the same way on every page.
 */
export const resolvedIdentity = (fields, { row, data, query }) => fields.map(field => row?.[field] ?? data?.[field] ?? query?.[field]);
