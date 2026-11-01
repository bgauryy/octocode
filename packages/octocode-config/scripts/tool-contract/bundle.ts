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
import { isObject, pascal, type Json, type JsonObject } from './json.ts';
import { nameVariants, typeName } from './variant-names.ts';

type ToolRecord = {
  name: string;
  querySchema: JsonObject;
  inputSchema: JsonObject;
  outputSchema: JsonObject;
};

/** Whether `value` is JSON all the way down, as the generated contract is. */
const isJsonValue = (value: unknown): value is Json =>
  value === null ||
  ['boolean', 'number', 'string'].includes(typeof value) ||
  (Array.isArray(value) && value.every(isJsonValue)) ||
  (typeof value === 'object' && Object.values(value).every(isJsonValue));

/** A contract value that must be a JSON object; anything else is a core defect. */
function jsonObject(value: unknown, what: string): JsonObject {
  if (isJsonValue(value) && isObject(value)) return value;
  throw new TypeError(`${what} is not a JSON object`);
}

/** Code-unit order, as `Array.prototype.sort` uses without a comparator. */
const byKey = ([left]: [string, unknown], [right]: [string, unknown]): number =>
  left < right ? -1 : left > right ? 1 : 0;

function sortKeys(value: Json): Json {
  if (Array.isArray(value)) return value.map(sortKeys);
  if (!isObject(value)) return value;
  return Object.fromEntries(
    Object.entries(value)
      .sort(byKey)
      .map(([key, child]) => [key, sortKeys(child)])
  );
}

function withoutAnnotations(value: Json): Json {
  if (Array.isArray(value)) return value.map(withoutAnnotations);
  return isObject(value) ? objectWithoutAnnotations(value) : value;
}

function objectWithoutAnnotations(value: JsonObject): JsonObject {
  return Object.fromEntries(
    Object.entries(value)
      .filter(([key]) => key !== 'title' && key !== 'description')
      .map(([key, child]) => [key, withoutAnnotations(child)])
  );
}

const shapeKey = (value: Json): string =>
  JSON.stringify(sortKeys(withoutAnnotations(value)));

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
    if (Array.isArray(value))
      return value.map(item => this.hoistTitles(item, false));
    if (!isObject(value)) return value;
    const walked = Object.fromEntries(
      Object.entries(value).map(([key, child]) => [
        key,
        this.hoistTitles(child, false),
      ])
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

  reuseQuery(inputName: string, queryName: string): void {
    const target = `#/$defs/${queryName}`;
    const resolve = (reference: string): Json | undefined =>
      reference.startsWith('#/$defs/')
        ? this.defs[reference.slice('#/$defs/'.length)]
        : undefined;
    const expand = (schema: Json, active = new Set<string>()): Json => {
      if (Array.isArray(schema))
        return schema.map(value => expand(value, active));
      if (!isObject(schema)) return schema;
      const clean = objectWithoutAnnotations(schema);
      const reference = clean.$ref;
      if (
        typeof reference === 'string' &&
        Object.keys(clean).length === 1 &&
        !active.has(reference)
      ) {
        const resolved = resolve(reference);
        if (resolved !== undefined)
          return expand(resolved, new Set([...active, reference]));
      }
      return Object.fromEntries(
        Object.entries(clean).map(([key, value]) => [
          key,
          expand(value, active),
        ])
      );
    };
    const query = this.defs[queryName];
    if (query === undefined) throw new Error(`Missing query definition: ${queryName}`);
    const expected = shapeKey(expand(query));
    const visited = new Set<JsonObject>();
    const visit = (schema: Json | undefined): void => {
      if (!isObject(schema) || visited.has(schema)) return;
      visited.add(schema);
      if (typeof schema.$ref === 'string') visit(resolve(schema.$ref));
      const queries = isObject(schema.properties)
        ? schema.properties.queries
        : undefined;
      const seenArrays = new Set<JsonObject>();
      const visitArray = (array: Json | undefined): void => {
        if (!isObject(array) || seenArrays.has(array)) return;
        seenArrays.add(array);
        if (typeof array.$ref === 'string') visitArray(resolve(array.$ref));
        if (array.items && shapeKey(expand(array.items)) === expected)
          array.items = { $ref: target };
      };
      visitArray(queries);
      for (const key of ['anyOf', 'oneOf', 'allOf']) {
        const branches = schema[key];
        if (Array.isArray(branches)) branches.forEach(visit);
      }
    };
    visit(this.defs[inputName]);
  }
}

/**
 * typify drops a string `const` (emitting a free `String`); a one-value
 * `enum` means the same and is enforced. Union keywords are left as authored:
 * closed objects can overlap, so `anyOf` is not `oneOf` (see `rustBundle`).
 * The compact IR omits implied primitive types; restore them for stable
 * generated names and typed fields instead of typify's generic Value fallback.
 */
function stringConstsAsEnums(value: Json): Json {
  if (Array.isArray(value)) return value.map(stringConstsAsEnums);
  if (!isObject(value)) return value;
  const walked = Object.fromEntries(
    Object.entries(value).map(([key, child]) => [
      key,
      stringConstsAsEnums(child),
    ])
  );
  if (walked.type === undefined) {
    const literals = Object.hasOwn(walked, 'const')
      ? [walked.const]
      : walked.enum;
    if (Array.isArray(literals) && literals.length) {
      const first = literals[0];
      if (first === null && literals.every(item => item === null))
        walked.type = 'null';
      else if (
        ['string', 'number', 'boolean'].includes(typeof first) &&
        literals.every(item => typeof item === typeof first)
      )
        walked.type = typeof first;
    }
  }
  if (typeof walked.const === 'string' && walked.enum === undefined) {
    const { const: constant, ...rest } = walked;
    return { ...rest, enum: [constant] };
  }
  return walked;
}

/** One IR tool record's name and schemas, checked as JSON. */
function toolRecord(record: Readonly<Record<string, unknown>>): ToolRecord {
  const { name } = record;
  if (typeof name !== 'string') throw new TypeError('A contract tool has no name');
  return {
    name,
    querySchema: jsonObject(record.querySchema, `${name}.querySchema`),
    inputSchema: jsonObject(record.inputSchema, `${name}.inputSchema`),
    outputSchema: jsonObject(record.outputSchema, `${name}.outputSchema`),
  };
}

export function buildToolTypesBundle(
  ir: EnforcementContractIr = buildEnforcementContractIr()
): {
  fingerprint: string;
  bundle: JsonObject;
} {
  const definitions = new Definitions();
  const register = (name: string, source: JsonObject): void => {
    const {
      $schema: _schema,
      $defs: inner = {},
      title: _title,
      ...body
    } = source;
    const normalize = (schema: Json, owner?: string): Json => {
      const exclusive = stringConstsAsEnums(schema);
      return definitions.hoistTitles(
        owner && isObject(exclusive)
          ? nameVariants(exclusive, owner)
          : exclusive
      );
    };
    for (const [local, schema] of Object.entries(isObject(inner) ? inner : {})) {
      // Zod emits `__schemaN` for a cyclic schema core left unnamed; a
      // generated type needs a stable, core-owned name.
      if (/^__schema\d+$/.test(local)) {
        throw new Error(
          `${name} has an unnamed recursive schema (${local}); name it in core with .meta({ id })`
        );
      }
      definitions.add(local, {
        ...jsonObject(normalize(schema), local),
        title: local,
      });
    }
    if (body.$ref === `#/$defs/${name}` && Object.keys(body).length === 1) {
      if (definitions.defs[name] === undefined) {
        throw new Error(`${name} references a missing root definition`);
      }
      return;
    }
    definitions.add(name, {
      ...jsonObject(normalize(body, name), name),
      title: name,
    });
  };
  register(
    'AnalysisCapabilities',
    jsonObject(ir.analysisCapabilitiesSchema, 'analysisCapabilitiesSchema')
  );
  for (const tool of ir.tools.map(toolRecord)) {
    const base = pascal(tool.name);
    register(`${base}Query`, tool.querySchema);
    register(`${base}Input`, tool.inputSchema);
    register(`${base}Output`, tool.outputSchema);
    // The bulk envelope repeats the row schema; reference the Query type so
    // both languages expose one row type, not two.
    definitions.reuseQuery(`${base}Input`, `${base}Query`);
  }
  return {
    fingerprint: ir.fingerprint,
    bundle: {
      $schema: 'https://json-schema.org/draft/2020-12/schema',
      $comment: `@generated by @octocodeai/config scripts/generate-tool-contract.ts from contract ${ir.fingerprint}; do not edit.`,
      title: 'OctocodeToolTypes',
      $defs: Object.fromEntries(
        Object.entries(definitions.defs).sort(byKey)
      ),
    },
  };
}
