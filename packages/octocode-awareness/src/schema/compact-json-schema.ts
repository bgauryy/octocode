type JsonObject = Record<string, unknown>;

const asObject = (value: unknown): JsonObject | undefined =>
  value && typeof value === 'object' && !Array.isArray(value)
    ? value as JsonObject
    : undefined;

const signature = (value: unknown): string => JSON.stringify(value);

/**
 * Hoist definitions shared by strict object-union branches. Branches retain their
 * own allowed-key sets, so additional-property and selector behavior stays exact.
 */
function hoistStrictObjectUnions(value: unknown): void {
  if (Array.isArray(value)) {
    for (const child of value) hoistStrictObjectUnions(child);
    return;
  }
  const object = asObject(value);
  if (!object) return;
  for (const child of Object.values(object)) hoistStrictObjectUnions(child);

  for (const unionKey of ['oneOf', 'anyOf'] as const) {
    const branches = Array.isArray(object[unionKey])
      ? object[unionKey].map(asObject)
      : [];
    if (branches.length < 2 || branches.some(branch =>
      !branch
      || branch['type'] !== 'object'
      || !asObject(branch['properties'])
      || branch['additionalProperties'] !== false
    )) continue;

    const schemasByProperty = new Map<string, unknown[]>();
    for (const branch of branches as JsonObject[]) {
      for (const [name, schema] of Object.entries(asObject(branch['properties'])!)) {
        const schemas = schemasByProperty.get(name) ?? [];
        schemas.push(schema);
        schemasByProperty.set(name, schemas);
      }
    }

    const properties: JsonObject = {};
    for (const [name, schemas] of schemasByProperty) {
      properties[name] = new Set(schemas.map(signature)).size === 1 ? schemas[0] : {};
    }

    const requiredSets = (branches as JsonObject[]).map(branch =>
      new Set(Array.isArray(branch['required']) ? branch['required'] as string[] : []),
    );
    const sharedRequired = [...requiredSets[0]!].filter(name =>
      requiredSets.every(required => required.has(name)),
    );

    // Every branch already enforces object type and strict keys; the root only owns
    // shared field constraints and requirements.
    delete object['type'];
    object['properties'] = properties;
    delete object['additionalProperties'];
    if (sharedRequired.length) object['required'] = sharedRequired;

    for (const branch of branches as JsonObject[]) {
      delete branch['$schema'];
      // The operation description owns route guidance; copied command summaries do not.
      delete branch['description'];
      const branchProperties = asObject(branch['properties'])!;
      for (const [name, schema] of Object.entries(branchProperties)) {
        if (signature(schema) === signature(properties[name])) branchProperties[name] = {};
      }
      if (Array.isArray(branch['required'])) {
        const required = (branch['required'] as string[]).filter(name => !sharedRequired.includes(name));
        if (required.length) branch['required'] = required;
        else delete branch['required'];
      }
    }
  }
}

function removeRedundantKeywords(value: unknown): void {
  if (Array.isArray(value)) {
    for (const child of value) removeRedundantKeywords(child);
    return;
  }
  const object = asObject(value);
  if (!object) return;
  delete object['$schema'];
  if (object['const'] !== undefined || Array.isArray(object['enum'])) delete object['type'];
  // Zod emits its date-time regular expression beside the equivalent JSON Schema format.
  if (object['format'] === 'date-time') delete object['pattern'];
  for (const child of Object.values(object)) removeRedundantKeywords(child);
}

interface PropertyOccurrence {
  name: string;
  schema: JsonObject;
  parents: JsonObject[];
}

/** Factor profitable exact property definitions into local, executable $defs. */
function factorRepeatedProperties(root: JsonObject): void {
  const occurrences = new Map<string, PropertyOccurrence>();
  const visit = (value: unknown): void => {
    if (Array.isArray(value)) {
      for (const child of value) visit(child);
      return;
    }
    const object = asObject(value);
    if (!object) return;
    const properties = asObject(object['properties']);
    if (properties) {
      for (const [name, valueSchema] of Object.entries(properties)) {
        const schema = asObject(valueSchema);
        if (!schema || Object.keys(schema).length === 0 || '$ref' in schema) continue;
        const key = `${name}\u0000${signature(schema)}`;
        const occurrence = occurrences.get(key) ?? { name, schema, parents: [] };
        occurrence.parents.push(properties);
        occurrences.set(key, occurrence);
        visit(schema);
      }
    }
    for (const [name, child] of Object.entries(object)) {
      if (name !== 'properties' && name !== '$defs') visit(child);
    }
  };
  visit(root);

  const definitions = asObject(root['$defs']) ?? {};
  let suffix = 1;
  for (const occurrence of [...occurrences.values()].sort((a, b) => b.parents.length - a.parents.length)) {
    if (occurrence.parents.length < 2) continue;
    let name = occurrence.name.replace(/[^A-Za-z0-9_.-]/g, '_') || 'value';
    while (definitions[name]) name = `${occurrence.name}_${suffix++}`;
    const reference = { $ref: `#/$defs/${name}` };
    const repeatedBytes = occurrence.parents.length * signature(occurrence.schema).length;
    const factoredBytes = signature(occurrence.schema).length
      + occurrence.parents.length * signature(reference).length
      + name.length + 3;
    if (factoredBytes >= repeatedBytes) continue;
    definitions[name] = occurrence.schema;
    for (const properties of occurrence.parents) properties[occurrence.name] = reference;
  }
  if (Object.keys(definitions).length) root['$defs'] = definitions;
}

/** Return a smaller, self-contained schema accepted by z.fromJSONSchema. */
export function compactAwarenessInputSchema(input: JsonObject): JsonObject {
  const output = structuredClone(input);
  hoistStrictObjectUnions(output);
  removeRedundantKeywords(output);
  factorRepeatedProperties(output);
  return output;
}
