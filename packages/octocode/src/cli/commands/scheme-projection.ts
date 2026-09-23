// Pure projections over the core-owned public tool catalog: the TS port of
// the retired Rust `cli/schema.rs` views. No I/O — the `scheme` command
// composes these with the native machine catalog.

import { usageLines } from './scheme-usage.js';

export { usageLines };

export type JsonValue =
  null | boolean | number | string | JsonValue[] | { [key: string]: JsonValue };
export type JsonObject = { [key: string]: JsonValue };

export type SchemeView = 'full' | 'query' | 'variants';

function cloneJson(value: JsonValue): JsonValue {
  return JSON.parse(JSON.stringify(value)) as JsonValue;
}

function jsonType(value: JsonValue | undefined): string {
  if (value === null) return 'null';
  if (Array.isArray(value)) return 'array';
  return typeof value;
}

/**
 * JSON Schema equates numeric spellings such as 1 and 1.0, including in
 * compound values. Only discard siblings when scalar disjointness is certain.
 */
function constsDisjoint(left: JsonValue, right: JsonValue): boolean {
  if (typeof left === 'string' && typeof right === 'string')
    return left !== right;
  if (typeof left === 'boolean' && typeof right === 'boolean')
    return left !== right;
  return jsonType(left) !== jsonType(right);
}

function deepEqual(left: JsonValue | undefined, right: JsonValue): boolean {
  return left !== undefined && JSON.stringify(left) === JSON.stringify(right);
}

function collectRefs(value: JsonValue | undefined, refs: string[]): void {
  if (Array.isArray(value)) {
    for (const entry of value) collectRefs(entry, refs);
    return;
  }
  if (!value || typeof value !== 'object') return;
  for (const [key, child] of Object.entries(value)) {
    if (
      (key === '$ref' || key === '$dynamicRef' || key === '$recursiveRef') &&
      typeof child === 'string'
    ) {
      refs.push(child);
    } else {
      collectRefs(child, refs);
    }
  }
}

function pruneUnreachableDefs(schema: JsonObject): void {
  const defs = schema.$defs;
  if (!defs || typeof defs !== 'object' || Array.isArray(defs)) return;
  const roots: JsonObject = { ...schema };
  delete roots.$defs;
  const pending: string[] = [];
  collectRefs(roots, pending);
  const needed = new Set<string>();
  while (pending.length > 0) {
    const reference = pending.pop() as string;
    // Anchor/external reference scopes can depend on definitions without a
    // JSON pointer. Keep all definitions when reachability is not provable.
    if (!reference.startsWith('#/$defs/')) return;
    const token = reference.slice('#/$defs/'.length).split('/')[0];
    if (token === undefined) return;
    const name = token.replaceAll('~1', '/').replaceAll('~0', '~');
    if (needed.has(name)) continue;
    needed.add(name);
    const definition = (defs as JsonObject)[name];
    if (definition === undefined) return;
    collectRefs(definition, pending);
  }
  if (needed.size === 0) {
    delete schema.$defs;
    return;
  }
  for (const name of Object.keys(defs as JsonObject)) {
    if (!needed.has(name)) delete (defs as JsonObject)[name];
  }
}

/**
 * The bulk inputSchema embeds querySchema verbatim as `queries.items` (with
 * its $defs hoisted). Replace that copy with a pointer so the full view does
 * not print the same schema twice; leave any non-identical envelope intact.
 */
function dedupeInputSchema(
  inputSchema: JsonValue | undefined,
  querySchema: JsonValue | undefined
): JsonValue | undefined {
  if (
    !inputSchema ||
    typeof inputSchema !== 'object' ||
    Array.isArray(inputSchema)
  )
    return inputSchema;
  if (
    !querySchema ||
    typeof querySchema !== 'object' ||
    Array.isArray(querySchema)
  )
    return inputSchema;
  const queries = (inputSchema.properties as JsonObject | undefined)?.queries;
  if (!queries || typeof queries !== 'object' || Array.isArray(queries))
    return inputSchema;
  const { $schema: _schema, $defs: queryDefs, ...queryBody } = querySchema;
  if (
    !deepEqual(queries.items, queryBody) ||
    JSON.stringify(inputSchema.$defs ?? null) !==
      JSON.stringify(queryDefs ?? null)
  )
    return inputSchema;
  const deduped = cloneJson(inputSchema) as JsonObject;
  const dedupedQueries = (deduped.properties as JsonObject)
    .queries as JsonObject;
  dedupedQueries.items = {
    description: 'Each item is one querySchema object.',
  };
  pruneUnreachableDefs(deduped);
  return deduped;
}

export function project(tool: JsonObject, view: SchemeView): JsonObject {
  if (view === 'full') {
    // Put branch selectors before the large schema so bounded renderers do not
    // hide the one-step route an agent needs to choose a union branch. `usage`
    // is the gh-CLI-style param cheat-sheet (mandatory <>, optional []) an agent
    // reads before the full schema.
    const {
      outputSchema: _outputSchema,
      name,
      variants,
      querySchema,
      ...published
    } = tool;
    const projected: JsonObject = {
      name,
      variants,
      usage: usageLines(tool),
      querySchema,
      ...published,
    };
    const inputSchema = dedupeInputSchema(published.inputSchema, querySchema);
    if (inputSchema !== undefined) projected.inputSchema = inputSchema;
    return projected;
  }
  if (view === 'variants') {
    const projected: JsonObject = { name: tool.name, variants: tool.variants };
    if (tool.description !== undefined)
      projected.description = tool.description;
    return projected;
  }
  // Keep the complete schema subtree: its local refs resolve against its
  // own root, including all core-owned $defs and validation constraints.
  const query: JsonObject = {
    name: tool.name,
    querySchema: cloneJson(tool.querySchema),
  };
  if (tool.description !== undefined) query.description = tool.description;
  const inputSchema = tool.inputSchema;
  const queries =
    inputSchema &&
    typeof inputSchema === 'object' &&
    !Array.isArray(inputSchema)
      ? (inputSchema as JsonObject).properties &&
        ((inputSchema as JsonObject).properties as JsonObject).queries
      : undefined;
  if (queries && typeof queries === 'object' && !Array.isArray(queries)) {
    const bounds: JsonObject = {};
    for (const key of ['minItems', 'maxItems']) {
      const bound = (queries as JsonObject)[key];
      if (bound !== undefined) bounds[key] = bound;
    }
    if (Object.keys(bounds).length > 0) {
      query.queryEnvelope = { queries: bounds };
    }
  }
  return query;
}

function parseSelection(selection: string): [string, JsonValue] {
  const separator = selection.indexOf('=');
  const field = separator < 0 ? '' : selection.slice(0, separator);
  const raw = separator < 0 ? '' : selection.slice(separator + 1);
  if (field.trim() === '' || raw.trim() === '') {
    throw new Error('--select expects FIELD=VALUE, e.g. operation=code');
  }
  let value: JsonValue;
  try {
    value = JSON.parse(raw) as JsonValue;
  } catch {
    value = raw;
  }
  return [field, value];
}

export function projectSelected(
  tool: JsonObject,
  view: SchemeView,
  selection: string | undefined
): JsonObject {
  if (selection === undefined) return project(tool, view);
  if (view !== 'query' || typeof tool.name !== 'string') {
    throw new Error('--select requires --view query and a tool name');
  }
  const [field, value] = parseSelection(selection);
  const projected = project(tool, view);
  const schema = projected.querySchema;
  if (!schema || typeof schema !== 'object' || Array.isArray(schema)) {
    throw new Error('Query schema must be an object');
  }
  const propertyConst = (
    branch: JsonValue,
    propertyName: string
  ): JsonValue | undefined => {
    if (!branch || typeof branch !== 'object' || Array.isArray(branch))
      return undefined;
    const properties = (branch as JsonObject).properties;
    if (
      !properties ||
      typeof properties !== 'object' ||
      Array.isArray(properties)
    ) {
      return undefined;
    }
    const property = (properties as JsonObject)[propertyName];
    if (!property || typeof property !== 'object' || Array.isArray(property)) {
      return undefined;
    }
    const reference = (property as JsonObject).$ref;
    if (typeof reference === 'string' && reference.startsWith('#/$defs/')) {
      const name = reference
        .slice('#/$defs/'.length)
        .replaceAll('~1', '/')
        .replaceAll('~0', '~');
      const definitions = (schema as JsonObject).$defs;
      if (
        definitions &&
        typeof definitions === 'object' &&
        !Array.isArray(definitions)
      ) {
        const target = (definitions as JsonObject)[name];
        if (target && typeof target === 'object' && !Array.isArray(target)) {
          return (target as JsonObject).const;
        }
      }
    }
    return (property as JsonObject).const;
  };
  const constOf = (branch: JsonValue): JsonValue | undefined =>
    propertyConst(branch, field);
  const variant =
    field === 'variant' &&
    typeof value === 'string' &&
    Array.isArray(tool.variants)
      ? tool.variants.find(
          candidate =>
            candidate !== null &&
            typeof candidate === 'object' &&
            !Array.isArray(candidate) &&
            (candidate as JsonObject).name === value
        )
      : undefined;
  if (field === 'variant' && variant === undefined) {
    const names = Array.isArray(tool.variants)
      ? tool.variants
          .filter(
            candidate =>
              candidate !== null &&
              typeof candidate === 'object' &&
              !Array.isArray(candidate) &&
              typeof (candidate as JsonObject).name === 'string'
          )
          .map(candidate => String((candidate as JsonObject).name))
      : [];
    throw new Error(
      `Unknown variant: ${String(value)}. Known variants: ${names.join(', ')}`
    );
  }
  const variantExample =
    variant !== undefined &&
    typeof variant === 'object' &&
    !Array.isArray(variant) &&
    (variant as JsonObject).example !== null &&
    typeof (variant as JsonObject).example === 'object' &&
    !Array.isArray((variant as JsonObject).example)
      ? ((variant as JsonObject).example as JsonObject)
      : undefined;
  const candidates: Array<['oneOf' | 'anyOf', number]> = [];
  for (const union of ['oneOf', 'anyOf'] as const) {
    const branches = (schema as JsonObject)[union];
    if (!Array.isArray(branches)) continue;
    branches.forEach((branch, index) => {
      if (variantExample) {
        const selectors = Object.entries(variantExample).filter(
          ([propertyName]) => propertyConst(branch, propertyName) !== undefined
        );
        if (
          selectors.length > 0 &&
          selectors.every(([propertyName, expected]) =>
            deepEqual(propertyConst(branch, propertyName), expected)
          )
        ) {
          candidates.push([union, index]);
        }
      } else if (deepEqual(constOf(branch), value)) {
        candidates.push([union, index]);
      }
    });
  }
  const allowMultiple = variantExample !== undefined;
  if (candidates.length === 0 || (!allowMultiple && candidates.length !== 1)) {
    throw new Error(
      `--select "${selection}" matched ${candidates.length} top-level oneOf/anyOf branches; choose a variant name or const field/value identifying one branch in --view query.`
    );
  }
  const union = candidates[0]![0];
  if (candidates.some(([candidateUnion]) => candidateUnion !== union)) {
    throw new Error(`--select "${selection}" matched multiple schema unions.`);
  }
  const branches = (schema as JsonObject)[union] as JsonValue[];
  const indexes = new Set(candidates.map(([, index]) => index));
  const selectedBranches = branches.filter((_, index) => indexes.has(index));
  const selected = selectedBranches[0];
  // Removing other oneOf branches must not admit instances that previously
  // matched multiple branches. Const discriminators usually prove disjointness;
  // retain exclusion constraints for siblings whose overlap cannot be ruled out.
  const requiresField = (branch: JsonValue): boolean => {
    if (!branch || typeof branch !== 'object' || Array.isArray(branch))
      return false;
    const required = (branch as JsonObject).required;
    return Array.isArray(required) && required.some(name => name === field);
  };
  const overlaps: JsonValue[] = [];
  if (union === 'oneOf' && !allowMultiple) {
    branches.forEach((branch, i) => {
      if (indexes.has(i)) return;
      const other = constOf(branch);
      const provablyDisjoint =
        other !== undefined &&
        constsDisjoint(other, value) &&
        (requiresField(selected) || requiresField(branch));
      if (!provablyDisjoint) overlaps.push(branch);
    });
  }
  (schema as JsonObject)[union] = selectedBranches;
  if (overlaps.length > 0) {
    const target = schema as JsonObject;
    if (!Array.isArray(target.allOf)) target.allOf = [];
    (target.allOf as JsonValue[]).push({ not: { anyOf: overlaps } });
  }
  pruneUnreachableDefs(schema as JsonObject);
  return projected;
}
