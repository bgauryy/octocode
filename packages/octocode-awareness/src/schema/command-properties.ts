type JsonObject = Record<string, unknown>;

const asObject = (value: unknown): JsonObject | undefined =>
  value && typeof value === 'object' && !Array.isArray(value)
    ? value as JsonObject
    : undefined;

function resolveRootProperty(schema: JsonObject, root: Readonly<JsonObject>): JsonObject {
  const reference = schema['$ref'];
  if (typeof reference !== 'string' || !reference.startsWith('#/$defs/')) return schema;
  const definitions = asObject(root['$defs']);
  return asObject(definitions?.[reference.slice('#/$defs/'.length)]) ?? schema;
}

/** Fields available at the request root, including discriminated command variants.
 * This is for binding/coercion only; validation keeps the complete union schema.
 */
export function commandSchemaProperties(
  schema: Readonly<JsonObject>,
): Record<string, JsonObject> {
  const properties: Record<string, JsonObject> = {};
  const collect = (current: Readonly<JsonObject>): void => {
    for (const [name, property] of Object.entries(asObject(current['properties']) ?? {})) {
      const propertySchema = asObject(property);
      if (!propertySchema) continue;
      const resolved = resolveRootProperty(propertySchema, schema);
      if (!properties[name] || Object.keys(resolved).length > 0) properties[name] = resolved;
    }
    for (const kind of ['oneOf', 'anyOf', 'allOf']) {
      const branches = current[kind];
      if (Array.isArray(branches)) {
        for (const branch of branches) {
          const branchSchema = asObject(branch);
          if (branchSchema) collect(branchSchema);
        }
      }
    }
  };
  collect(schema);
  return properties;
}
