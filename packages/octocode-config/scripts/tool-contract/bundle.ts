// Bundles every tool's query/input/output JSON Schema into one document whose
// `$defs` names are the generated type names in both TypeScript and Rust.
//
// Naming comes only from core: a schema core names — a `.meta({ id })` `$def`
// or a `.meta({ title })` — is one global type; everything else is named by its
// position (`<Tool>Query` + property path), so a contract change never renames
// an unrelated type. Two different shapes under one core name fail generation.
import {
  buildEnforcementContractIr,
  type EnforcementContractIr,
} from '@octocodeai/octocode-core/schema';

export type Json = null | boolean | number | string | Json[] | { [key: string]: Json };
export type JsonObject = { [key: string]: Json };

type ToolRecord = {
  name: string;
  querySchema: JsonObject;
  inputSchema: JsonObject;
  outputSchema: JsonObject;
};

const isObject = (value: Json | undefined): value is JsonObject =>
  !!value && typeof value === 'object' && !Array.isArray(value);

export const pascal = (value: string): string => value.charAt(0).toUpperCase() + value.slice(1);

/** `workspace:uri` → `WorkspaceUri`, `anchored` → `Anchored`. */
const typeName = (title: string): string =>
  title
    .split(/[^A-Za-z0-9]+/)
    .filter(Boolean)
    .map(pascal)
    .join('');

function sortKeys(value: Json): Json {
  if (Array.isArray(value)) return value.map(sortKeys);
  if (!isObject(value)) return value;
  return Object.fromEntries(
    Object.keys(value)
      .sort()
      .map((key) => [key, sortKeys(value[key] as Json)])
  );
}

function withoutAnnotations(value: Json): Json {
  if (Array.isArray(value)) return value.map(withoutAnnotations);
  if (!isObject(value)) return value;
  return Object.fromEntries(
    Object.entries(value)
      .filter(([key]) => key !== 'title' && key !== 'description')
      .map(([key, child]) => [key, withoutAnnotations(child)])
  );
}

const shapeKey = (value: Json): string => JSON.stringify(sortKeys(withoutAnnotations(value)));

class Definitions {
  readonly defs: Record<string, Json> = {};

  add(name: string, schema: Json): void {
    const existing = this.defs[name];
    if (existing !== undefined && shapeKey(existing) !== shapeKey(schema)) {
      throw new Error(`Conflicting tool-type definition: ${name}`);
    }
    this.defs[name] ??= schema;
  }

  /**
   * Replaces every titled subschema with a `$ref` to one global definition
   * named by the title; use-site annotations stay at the use site.
   */
  hoistTitles(value: Json, root = true): Json {
    if (Array.isArray(value)) return value.map((item) => this.hoistTitles(item, false));
    if (!isObject(value)) return value;
    const walked = Object.fromEntries(
      Object.entries(value).map(([key, child]) => [key, this.hoistTitles(child, false)])
    );
    if (root || typeof walked.title !== 'string') return walked;
    const name = typeName(walked.title);
    const { title: _title, description, default: fallback, ...shape } = walked;
    this.add(name, { title: name, ...shape });
    return {
      $ref: `#/$defs/${name}`,
      ...(description === undefined ? {} : { description }),
      ...(fallback === undefined ? {} : { default: fallback }),
    };
  }
}

/**
 * Rewrites for typify, which drops a string `const` (emitting a free
 * `String`) and flattens `anyOf` into a struct of optional subtypes:
 * - a string `const` becomes a one-value `enum`, which it enforces;
 * - `anyOf` over closed objects admits exactly one branch, so it becomes
 *   `oneOf`, which it emits as an enum.
 */
function exclusiveUnions(value: Json): Json {
  if (Array.isArray(value)) return value.map(exclusiveUnions);
  if (!isObject(value)) return value;
  const walked = Object.fromEntries(
    Object.entries(value).map(([key, child]) => [key, exclusiveUnions(child)])
  );
  if (typeof walked.const === 'string' && walked.enum === undefined) {
    const { const: constant, ...rest } = walked;
    return { ...rest, enum: [constant] };
  }
  const branches = walked.anyOf;
  if (
    Array.isArray(branches) &&
    branches.length > 1 &&
    branches.every((branch) => isObject(branch) && branch.type === 'object' && branch.additionalProperties === false)
  ) {
    const { anyOf: _anyOf, ...rest } = walked;
    return { ...rest, oneOf: branches };
  }
  return walked;
}

const singleValue = (schema: Json | undefined): string | undefined =>
  isObject(schema) && Array.isArray(schema.enum) && schema.enum.length === 1 && typeof schema.enum[0] === 'string'
    ? schema.enum[0]
    : undefined;

/**
 * typify tags a `oneOf` by a discriminator whose values are unique; when two
 * branches share a value (astSearch's two `operation: "match"` forms) it falls
 * back to positional `Variant0…` names. Name those branches from the contract
 * itself instead: `<Owner><Value>`, plus the required field only that branch
 * has when values repeat (`AstSearchQueryMatchPattern` / `…MatchRule`).
 */
function nameUntaggableVariants(schema: JsonObject, owner: string): JsonObject {
  const branches = schema.oneOf;
  if (!Array.isArray(branches) || !branches.every((branch) => isObject(branch) && branch.title === undefined)) {
    return schema;
  }
  const objects = branches as JsonObject[];
  const properties = (branch: JsonObject): JsonObject => (isObject(branch.properties) ? branch.properties : {});
  const discriminators = Object.keys(properties(objects[0] ?? {})).filter((key) =>
    objects.every((branch) => singleValue(properties(branch)[key]) !== undefined)
  );
  const valuesOf = (key: string): string[] =>
    objects.map((branch) => singleValue(properties(branch)[key]) as string);
  // Any discriminator with unique values lets typify tag the enum itself.
  if (discriminators.some((key) => new Set(valuesOf(key)).size === objects.length)) return schema;
  const [discriminator] = discriminators;
  if (!discriminator) return schema;
  const values = valuesOf(discriminator);
  const required = (branch: JsonObject): string[] =>
    Array.isArray(branch.required) ? branch.required.filter((key): key is string => typeof key === 'string') : [];
  const titled = objects.map((branch, index) => {
    const value = values[index] as string;
    const peers = objects.filter((_, other) => other !== index && values[other] === value);
    const own = required(branch).find((key) => peers.every((peer) => !required(peer).includes(key)));
    const suffix = peers.length === 0 ? '' : own ? typeName(own) : String(index);
    return { ...branch, title: `${owner}${typeName(value)}${suffix}` };
  });
  return { ...schema, oneOf: titled };
}

export function buildToolTypesBundle(ir: EnforcementContractIr = buildEnforcementContractIr()): {
  fingerprint: string;
  bundle: JsonObject;
} {
  const definitions = new Definitions();
  const register = (name: string, source: JsonObject): void => {
    const { $schema: _schema, $defs: inner = {}, title: _title, ...body } = source;
    const normalize = (schema: Json, owner?: string): Json => {
      const exclusive = exclusiveUnions(schema);
      return definitions.hoistTitles(owner && isObject(exclusive) ? nameUntaggableVariants(exclusive, owner) : exclusive);
    };
    for (const [local, schema] of Object.entries(inner as JsonObject)) {
      // Zod emits `__schemaN` for a cyclic schema core left unnamed; a
      // generated type needs a stable, core-owned name.
      if (/^__schema\d+$/.test(local)) {
        throw new Error(`${name} has an unnamed recursive schema (${local}); name it in core with .meta({ id })`);
      }
      definitions.add(local, { ...(normalize(schema) as JsonObject), title: local });
    }
    definitions.add(name, { ...(normalize(body, name) as JsonObject), title: name });
  };
  for (const tool of ir.tools as unknown as ToolRecord[]) {
    const base = pascal(tool.name);
    register(`${base}Query`, tool.querySchema);
    register(`${base}Input`, tool.inputSchema);
    register(`${base}Output`, tool.outputSchema);
    // The bulk envelope repeats the row schema; reference the Query type so
    // both languages expose one row type, not two.
    const input = definitions.defs[`${base}Input`] as JsonObject;
    const queries = (input.properties as JsonObject | undefined)?.queries;
    if (isObject(queries) && queries.items && shapeKey(queries.items) === shapeKey(definitions.defs[`${base}Query`] as Json)) {
      queries.items = { $ref: `#/$defs/${base}Query` };
    }
  }
  const names = Object.keys(definitions.defs).sort();
  return {
    fingerprint: ir.fingerprint,
    bundle: {
      $schema: 'https://json-schema.org/draft/2020-12/schema',
      $comment: `@generated by @octocodeai/config scripts/generate-tool-contract.ts from contract ${ir.fingerprint}; do not edit.`,
      title: 'OctocodeToolTypes',
      $defs: Object.fromEntries(names.map((name) => [name, definitions.defs[name] as Json])),
    },
  };
}
