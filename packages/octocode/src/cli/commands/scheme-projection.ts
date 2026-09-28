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

export function project(tool: JsonObject, view: SchemeView): JsonObject {
  if (view === 'full') {
    // Selectors, examples (the only guide for single-shape tools), and the
    // gh-CLI-style `usage` precede the large schema so bounded renderers keep them.
    const {
      outputSchema: _outputSchema,
      name,
      variants,
      examples,
      querySchema: _querySchema,
      ...published
    } = tool;
    const projected: JsonObject = {
      name,
      variants,
      ...(examples !== undefined ? { examples } : {}),
      usage: usageLines(tool),
      ...published,
    };
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
  const [field, selectedValue] = parseSelection(selection);
  let value = selectedValue;
  const projected = project(tool, view);
  const schema = projected.querySchema;
  if (!schema || typeof schema !== 'object' || Array.isArray(schema)) {
    throw new Error('Query schema must be an object');
  }
  const propertySchema = (
    branch: JsonValue,
    propertyName: string
  ): JsonObject | undefined => {
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
    let resolved = property as JsonObject;
    const seen = new Set<string>();
    while (typeof resolved.$ref === 'string') {
      const reference = resolved.$ref;
      if (!reference.startsWith('#/') || seen.has(reference)) return undefined;
      seen.add(reference);
      let target: JsonValue | undefined = schema;
      for (const token of reference.slice(2).split('/')) {
        if (!target || typeof target !== 'object' || Array.isArray(target))
          return undefined;
        target = (target as JsonObject)[
          token.replaceAll('~1', '/').replaceAll('~0', '~')
        ];
      }
      if (!target || typeof target !== 'object' || Array.isArray(target))
        return undefined;
      // Ref siblings remain constraints; default is annotation, never a selector.
      resolved = { ...target, ...resolved };
      delete resolved.$ref;
      if (typeof (target as JsonObject).$ref === 'string')
        resolved.$ref = (target as JsonObject).$ref!;
    }
    return resolved;
  };
  const propertyConst = (
    branch: JsonValue,
    name: string
  ): JsonValue | undefined => propertySchema(branch, name)?.const;
  const constOf = (branch: JsonValue): JsonValue | undefined =>
    propertyConst(branch, field);
  const accepts = (branch: JsonValue, target: JsonValue): boolean => {
    const property = propertySchema(branch, field);
    if (property?.const !== undefined) return deepEqual(property.const, target);
    return (
      Array.isArray(property?.enum) &&
      property.enum.some(item => deepEqual(item, target))
    );
  };
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
  // Catalog labels like `match(pattern)` name a const plus a required field.
  const labelled =
    variantExample === undefined && typeof value === 'string'
      ? /^([^()]+)\(([^()]+)\)$/.exec(value)
      : null;
  const requires = (branch: JsonValue, name: string): boolean => {
    const required = (branch as JsonObject | null)?.required;
    return Array.isArray(required) && required.includes(name);
  };
  const variantTitle =
    variant && typeof variant === 'object' && !Array.isArray(variant)
      ? variant.name
      : undefined;
  const hasNamedBranch =
    variantTitle !== undefined &&
    ['oneOf', 'anyOf'].some(union => {
      const branches = (schema as JsonObject)[union];
      return (
        Array.isArray(branches) &&
        branches.some(
          branch =>
            branch &&
            typeof branch === 'object' &&
            !Array.isArray(branch) &&
            branch.title === variantTitle
        )
      );
    });
  const matchesVariant = (branch: JsonValue): boolean => {
    if (hasNamedBranch) return (branch as JsonObject).title === variantTitle;
    const selectors = Object.entries(variantExample ?? {}).filter(
      ([name]) => propertyConst(branch, name) !== undefined
    );
    const same = ([name, expected]: [string, JsonValue]) =>
      deepEqual(propertyConst(branch, name), expected);
    return selectors.length > 0 && selectors.every(same);
  };
  const collect = (target: JsonValue, requiredField?: string) => {
    const found: Array<['oneOf' | 'anyOf', number]> = [];
    for (const union of ['oneOf', 'anyOf'] as const) {
      const branches = (schema as JsonObject)[union];
      if (!Array.isArray(branches)) continue;
      branches.forEach((branch, index) => {
        const hit = variantExample
          ? matchesVariant(branch)
          : accepts(branch, target) &&
            (requiredField === undefined || requires(branch, requiredField));
        if (hit) found.push([union, index]);
      });
    }
    return found;
  };
  let candidates = collect(value);
  if (candidates.length === 0 && labelled) {
    value = labelled[1]!.trim();
    candidates = collect(value, labelled[2]!.trim());
  }
  // Every branch sharing the selected const is a valid slice (operation=match
  // keeps both the pattern and rule shapes); only an empty match is an error.
  if (candidates.length === 0) {
    throw new Error(
      `--select "${selection}" matched 0 top-level oneOf/anyOf branches; choose a variant name or const/enum field value identifying a branch in --view query.`
    );
  }
  const union = candidates[0]![0];
  if (candidates.some(([candidateUnion]) => candidateUnion !== union)) {
    throw new Error(`--select "${selection}" matched multiple schema unions.`);
  }
  const branches = (schema as JsonObject)[union] as JsonValue[];
  const indexes = new Set(candidates.map(([, index]) => index));
  const selectedBranches = branches.filter((_, index) => indexes.has(index));
  // Removing other oneOf branches must not admit instances that previously
  // matched multiple branches. Const discriminators usually prove disjointness;
  // retain exclusion constraints for siblings whose overlap cannot be ruled out.
  const overlaps: JsonValue[] = [];
  if (union === 'oneOf' && variantExample === undefined) {
    branches.forEach((branch, i) => {
      if (indexes.has(i)) return;
      const other = constOf(branch);
      const provablyDisjoint =
        other !== undefined &&
        selectedBranches.every(selected => {
          const selectedConst = constOf(selected);
          return (
            selectedConst !== undefined && constsDisjoint(other, selectedConst)
          );
        }) &&
        (selectedBranches.every(selected => requires(selected, field)) ||
          requires(branch, field));
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
