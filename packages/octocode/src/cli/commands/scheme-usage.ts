import type { JsonObject, JsonValue } from './scheme-projection.js';

/** Resolve a scalar `const`, following a local `$defs` reference when needed. */
function propConst(prop: JsonValue, defs: JsonObject): JsonValue | undefined {
  if (!prop || typeof prop !== 'object' || Array.isArray(prop))
    return undefined;
  const direct = (prop as JsonObject).const;
  if (direct !== undefined) return direct;
  const ref = (prop as JsonObject).$ref;
  if (typeof ref === 'string' && ref.startsWith('#/$defs/')) {
    const key = ref
      .slice('#/$defs/'.length)
      .replaceAll('~1', '/')
      .replaceAll('~0', '~');
    const target = defs[key];
    if (target && typeof target === 'object' && !Array.isArray(target)) {
      return (target as JsonObject).const;
    }
  }
  return undefined;
}

function scalarConst(
  props: JsonObject,
  key: string,
  defs: JsonObject
): string | undefined {
  const value = propConst(props[key], defs);
  return typeof value === 'string' ||
    typeof value === 'number' ||
    typeof value === 'boolean'
    ? String(value)
    : undefined;
}

function branchProps(branch: JsonObject): JsonObject {
  return branch.properties &&
    typeof branch.properties === 'object' &&
    !Array.isArray(branch.properties)
    ? (branch.properties as JsonObject)
    : {};
}

/**
 * Return the branch's scalar discriminator as `[property, value]`: the first
 * const whose value differs across sibling branches, else the first const.
 */
function discriminator(
  props: JsonObject,
  required: readonly string[],
  defs: JsonObject,
  siblings: readonly JsonObject[]
): [string, string] {
  const keys = required.length > 0 ? required : Object.keys(props);
  let first: [string, string] | undefined;
  for (const key of keys) {
    const value = scalarConst(props, key, defs);
    if (value === undefined) continue;
    first ??= [key, value];
    if (
      siblings.some(
        other => scalarConst(branchProps(other), key, defs) !== value
      )
    ) {
      return [key, value];
    }
  }
  return first ?? ['', ''];
}

/** Set by the runtime on emitted next.* continuations, never typed by hand. */
const RUNTIME_SET_FIELDS = new Set(['followUp']);
/** A new query states its brief; only a followUp continuation inherits it. */
const NEW_QUERY_BRIEF = ['goal', 'reasoning'];

function branchUsage(
  branch: JsonObject,
  defs: JsonObject,
  siblings: readonly JsonObject[]
): string {
  const props = branchProps(branch);
  const declared = Array.isArray(branch.required)
    ? branch.required.map(String)
    : [];
  const required =
    'followUp' in props
      ? [
          ...NEW_QUERY_BRIEF.filter(
            name => name in props && !declared.includes(name)
          ),
          ...declared,
        ]
      : declared;
  const [labelKey, labelValue] = discriminator(props, required, defs, siblings);
  const mandatory = required
    .filter(name => name !== labelKey)
    .map(name => `<${name}>`)
    .join(' ');
  const optional = Object.keys(props)
    .filter(name => !required.includes(name) && !RUNTIME_SET_FIELDS.has(name))
    .map(name => `[${name}]`)
    .join(' ');
  const fields = [mandatory, optional].filter(Boolean).join(' ');
  return labelKey ? `${labelKey}=${labelValue}  ${fields}`.trim() : fields;
}

/** Render a query schema as compact gh-CLI-style usage lines. */
export function usageLines(tool: JsonObject): string[] {
  const name = typeof tool.name === 'string' ? tool.name : 'tool';
  const invocation = `octocode ${name} '{"queries":[ … ]}'`;
  const schema = tool.querySchema;
  if (!schema || typeof schema !== 'object' || Array.isArray(schema)) {
    return [invocation, 'see: scheme ' + name + ' --view query'];
  }

  const projected = schema as JsonObject;
  const union = Array.isArray(projected.oneOf)
    ? (projected.oneOf as JsonValue[])
    : Array.isArray(projected.anyOf)
      ? (projected.anyOf as JsonValue[])
      : null;
  const branches: JsonObject[] = union
    ? union.filter(
        (branch): branch is JsonObject =>
          !!branch && typeof branch === 'object' && !Array.isArray(branch)
      )
    : [projected];
  const defs =
    projected.$defs &&
    typeof projected.$defs === 'object' &&
    !Array.isArray(projected.$defs)
      ? (projected.$defs as JsonObject)
      : {};
  const lines = branches
    .map(branch => branchUsage(branch, defs, branches))
    .filter(line => line.length > 0);
  const unique = [...new Set(lines)];
  return unique.length > 0
    ? [invocation, ...unique]
    : [invocation, 'see: scheme ' + name + ' --view query'];
}
