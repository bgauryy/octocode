// Pure projections over the core-owned public tool catalog: the TS port of
// the retired Rust `cli/schema.rs` views. No I/O — the `scheme` command
// composes these with the native machine catalog.

export type JsonValue =
  null | boolean | number | string | JsonValue[] | { [key: string]: JsonValue };
export type JsonObject = { [key: string]: JsonValue };

export type SchemeView = 'full' | 'query';

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

export function project(tool: JsonObject, view: SchemeView): JsonObject {
  if (view === 'full') {
    // The public catalog never carries outputSchema; drop defensively anyway.
    const { outputSchema: _outputSchema, ...published } = tool;
    return published;
  }
  // Keep the complete schema subtree: its local refs resolve against its
  // own root, including all core-owned $defs and validation constraints.
  const query: JsonObject = { name: tool.name, querySchema: tool.querySchema };
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
  const constOf = (branch: JsonValue): JsonValue | undefined => {
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
    const property = (properties as JsonObject)[field];
    if (!property || typeof property !== 'object' || Array.isArray(property)) {
      return undefined;
    }
    return (property as JsonObject).const;
  };
  const candidates: Array<['oneOf' | 'anyOf', number]> = [];
  for (const union of ['oneOf', 'anyOf'] as const) {
    const branches = (schema as JsonObject)[union];
    if (!Array.isArray(branches)) continue;
    branches.forEach((branch, index) => {
      if (deepEqual(constOf(branch), value)) candidates.push([union, index]);
    });
  }
  if (candidates.length !== 1) {
    throw new Error(
      `--select "${selection}" matched ${candidates.length} top-level oneOf/anyOf branches; choose a const field/value identifying exactly one branch in --view query.`
    );
  }
  const [union, index] = candidates[0];
  const branches = (schema as JsonObject)[union] as JsonValue[];
  const selected = branches[index];
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
  if (union === 'oneOf') {
    branches.forEach((branch, i) => {
      if (i === index) return;
      const other = constOf(branch);
      const provablyDisjoint =
        other !== undefined &&
        constsDisjoint(other, value) &&
        (requiresField(selected) || requiresField(branch));
      if (!provablyDisjoint) overlaps.push(branch);
    });
  }
  (schema as JsonObject)[union] = [selected];
  if (overlaps.length > 0) {
    const target = schema as JsonObject;
    if (!Array.isArray(target.allOf)) target.allOf = [];
    (target.allOf as JsonValue[]).push({ not: { anyOf: overlaps } });
  }
  pruneUnreachableDefs(schema as JsonObject);
  return projected;
}
