// Names the branches of a generated union from the contract itself, so
// typify emits `<Owner><Value>` types instead of positional `Variant0…`.
import { isObject, pascal, type Json, type JsonObject } from './json.ts';

/** `workspace:uri` → `WorkspaceUri`, `anchored` → `Anchored`. */
export const typeName = (title: string): string =>
  title
    .split(/[^A-Za-z0-9]+/)
    .filter(Boolean)
    .map(pascal)
    .join('');

/** The union keyword a schema branches on, as authored. */
const unionKey = (schema: JsonObject): 'oneOf' | 'anyOf' | undefined =>
  Array.isArray(schema.oneOf)
    ? 'oneOf'
    : Array.isArray(schema.anyOf)
      ? 'anyOf'
      : undefined;

const singleValue = (schema: Json | undefined): string | undefined =>
  isObject(schema) &&
  Array.isArray(schema.enum) &&
  schema.enum.length === 1 &&
  typeof schema.enum[0] === 'string'
    ? schema.enum[0]
    : undefined;

/**
 * typify tags a `oneOf` by a required discriminator whose values are unique;
 * when two branches share a value (astSearch's two `operation: "match"`
 * forms) or the discriminator is optional, it falls back to positional
 * `Variant0…` names. Name those branches from the contract
 * itself instead: `<Owner><Value>`, plus the required field only that branch
 * has when values repeat (`AstSearchQueryMatchPattern` / `…MatchRule`).
 */
export function nameVariants(schema: JsonObject, owner: string): JsonObject {
  const key = unionKey(schema);
  const branches = key ? schema[key] : undefined;
  if (
    !key ||
    !Array.isArray(branches) ||
    !branches.every(branch => isObject(branch) && branch.title === undefined)
  ) {
    return schema;
  }
  const objects = branches.filter(isObject);
  const candidates = [...new Set(objects.flatMap(branch => Object.keys(properties(branch))))];
  // A discriminator tells branches apart: every branch holds one value of
  // it, and not all the same one.
  const discriminators = candidates.filter(
    field =>
      objects.every(branch => branchValue(branch, field) !== undefined) &&
      new Set(objects.map(branch => branchValue(branch, field))).size > 1
  );
  const [discriminator] = discriminators;
  if (!discriminator)
    return nameByOwnField(objects, owner, schema, key);
  const values = objects.map((branch, index) => branchValue(branch, discriminator) ?? String(index));
  const titled = objects.map((branch, index) => {
    const value = values[index] ?? String(index);
    const peers = objects.filter(
      (_, other) => other !== index && values[other] === value
    );
    const own = required(branch).find(field =>
      peers.every(peer => !required(peer).includes(field))
    );
    const suffix =
      peers.length === 0 ? '' : own ? typeName(own) : String(index);
    const title = `${owner}${typeName(value)}${suffix}`;
    // A branch that is itself a union names its own forms under its title.
    return { ...nameVariants(branch, title), title };
  });
  return { ...schema, [key]: titled };
}

const properties = (branch: JsonObject): JsonObject =>
  isObject(branch.properties) ? branch.properties : {};

const required = (branch: JsonObject): string[] =>
  Array.isArray(branch.required)
    ? branch.required.filter((key): key is string => typeof key === 'string')
    : [];

/**
 * The one literal `branch` holds for `key`: its own, or the one every form
 * of its nested union shares (graphQuery's three `impact` forms).
 */
function branchValue(branch: JsonObject, key: string): string | undefined {
  const own = singleValue(properties(branch)[key]);
  if (own !== undefined) return own;
  const nestedKey = unionKey(branch);
  const nested = nestedKey ? branch[nestedKey] : undefined;
  if (!Array.isArray(nested) || nested.length === 0) return undefined;
  const values = new Set(
    nested.map(form => (isObject(form) ? branchValue(form, key) : undefined))
  );
  const [only] = values;
  return values.size === 1 ? only : undefined;
}

/**
 * Field-discriminated branches use their first unique required field:
 * `<Owner><Field>`.
 */
function nameByOwnField(
  objects: JsonObject[],
  owner: string,
  schema: JsonObject,
  key: 'oneOf' | 'anyOf'
): JsonObject {
  const titled: JsonObject[] = [];
  for (const [index, branch] of objects.entries()) {
    const peers = objects.filter((_, other) => other !== index);
    const own = required(branch).find(field =>
      peers.every(peer => !required(peer).includes(field))
    );
    if (own === undefined) return schema;
    titled.push({ ...branch, title: `${owner}${typeName(own)}` });
  }
  if (new Set(titled.map(branch => branch.title)).size !== titled.length)
    return schema;
  return { ...schema, [key]: titled };
}
