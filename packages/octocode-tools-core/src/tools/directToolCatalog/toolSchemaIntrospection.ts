/**
 * Engine-free JSON-schema introspection: display fields, constraint text,
 * and example values.
 */
import { z } from 'zod';
import {
  DIRECT_TOOL_AUTO_FILLED_FIELDS,
  findDirectToolDefinition,
  type DirectToolDisplayField,
} from './toolCatalogDefinitions.js';

interface JsonSchemaObject extends Record<string, unknown> {
  type?: string | string[];
  description?: string;
  enum?: unknown[];
  required?: string[];
  properties?: Record<string, unknown>;
  items?: unknown;
  anyOf?: unknown[];
  oneOf?: unknown[];
  const?: unknown;
}

export function getDirectToolDisplayFields(
  toolName: string
): DirectToolDisplayField[] {
  const tool = findDirectToolDefinition(toolName);
  if (!tool) {
    return [];
  }

  const jsonSchema = z.toJSONSchema(tool.schema, { io: 'input' });
  if (!isJsonSchemaObject(jsonSchema)) {
    return [];
  }

  const variants = collectObjectVariants(jsonSchema);
  const properties = mergeVariantProperties(variants);
  const requiredFields = intersectRequiredFields(variants, properties);

  return collectDisplayFields(properties, requiredFields);
}

/** Exact per-operation fields for discriminated schemas. Unlike the merged
 * display view, this preserves branch-specific enums and numeric limits. */
export function getDirectToolVariantDisplayFields(
  toolName: string
): Record<string, DirectToolDisplayField[]> {
  const tool = findDirectToolDefinition(toolName);
  if (!tool) return {};

  const jsonSchema = z.toJSONSchema(tool.schema, { io: 'input' });
  if (!isJsonSchemaObject(jsonSchema)) return {};

  const byOperation = new Map<string, JsonSchemaObject[]>();
  for (const variant of collectObjectVariants(jsonSchema)) {
    if (!isRecord(variant.properties)) continue;
    const operation = variant.properties.operation;
    if (!isJsonSchemaObject(operation) || typeof operation.const !== 'string')
      continue;
    const variants = byOperation.get(operation.const) ?? [];
    variants.push(variant);
    byOperation.set(operation.const, variants);
  }

  return Object.fromEntries(
    [...byOperation].map(([operation, variants]) => {
      const properties = mergeVariantProperties(variants);
      const required = intersectRequiredFields(variants, properties);
      return [
        operation,
        collectDisplayFields(properties, required).filter(
          field =>
            !DIRECT_TOOL_AUTO_FILLED_FIELDS.has(field.name) &&
            field.name !== 'operation'
        ),
      ];
    })
  );
}

/** Top-level fields accepted by the schema branches compatible with the
 * selectors already present in a query. This keeps recovery suggestions inside
 * the active discriminated-union branch instead of the merged catalog view. */
export function getDirectToolAllowedFieldNames(
  toolName: string,
  query: Readonly<Record<string, unknown>>
): ReadonlySet<string> {
  const tool = findDirectToolDefinition(toolName);
  if (!tool) return new Set();

  const jsonSchema = z.toJSONSchema(tool.schema, { io: 'input' });
  if (!isJsonSchemaObject(jsonSchema)) return new Set();
  const variants = collectObjectVariants(jsonSchema);
  const compatible = variants.filter(variant =>
    branchMatchesKnownSelectors(variant, query)
  );
  const selected = compatible.length > 0 ? compatible : variants;
  return new Set(
    selected.flatMap(variant =>
      isRecord(variant.properties) ? Object.keys(variant.properties) : []
    )
  );
}

function branchMatchesKnownSelectors(
  variant: JsonSchemaObject,
  query: Readonly<Record<string, unknown>>
): boolean {
  if (!isRecord(variant.properties)) return true;
  for (const [name, value] of Object.entries(query)) {
    const rawProperty = variant.properties[name];
    if (!isJsonSchemaObject(rawProperty)) continue;
    if ('const' in rawProperty && value !== rawProperty.const) return false;
    if (
      Array.isArray(rawProperty.enum) &&
      !rawProperty.enum.some(candidate => candidate === value)
    ) {
      return false;
    }
  }
  return true;
}

export function describeSchemaConstraints(
  schema: JsonSchemaObject
): string | undefined {
  const parts: string[] = [];
  const min = typeof schema.minimum === 'number' ? schema.minimum : undefined;
  const max = typeof schema.maximum === 'number' ? schema.maximum : undefined;
  if (min !== undefined && max !== undefined) parts.push(`${min}-${max}`);
  else if (min !== undefined) parts.push(`>=${min}`);
  else if (max !== undefined) parts.push(`<=${max}`);
  if ('default' in schema)
    parts.push(`default ${JSON.stringify(schema.default)}`);
  return parts.length > 0 ? parts.join(', ') : undefined;
}

export function describeSchemaType(schema: JsonSchemaObject): string {
  if ('const' in schema) {
    return `enum(${String(schema.const)})`;
  }
  if (Array.isArray(schema.enum) && schema.enum.length > 0) {
    return `enum(${schema.enum.map(String).join(', ')})`;
  }

  if (schema.type === 'array') {
    const items = isJsonSchemaObject(schema.items) ? schema.items : undefined;
    return `array<${items ? describeSchemaType(items) : 'value'}>`;
  }

  // Unions (z.union → anyOf, z.discriminatedUnion → oneOf) carry no top-level
  // `type`, which would otherwise fall through to the opaque "value". Render the
  // member types instead, e.g. `string | array<string>`.
  const union = Array.isArray(schema.anyOf)
    ? schema.anyOf
    : Array.isArray(schema.oneOf)
      ? schema.oneOf
      : undefined;
  if (union) {
    const members = union
      .filter(isJsonSchemaObject)
      .map(describeSchemaType)
      .filter(t => t !== 'value');
    if (members.length > 0) return [...new Set(members)].join(' | ');
  }

  if (Array.isArray(schema.type)) {
    return schema.type.join(' | ');
  }

  if (typeof schema.type === 'string') {
    return schema.type;
  }

  return 'value';
}

function collectObjectVariants(schema: JsonSchemaObject): JsonSchemaObject[] {
  if (isRecord(schema.properties)) return [schema];
  const union = Array.isArray(schema.anyOf)
    ? schema.anyOf
    : Array.isArray(schema.oneOf)
      ? schema.oneOf
      : [];
  return union
    .filter(isJsonSchemaObject)
    .flatMap(member => collectObjectVariants(member));
}

function mergeVariantProperties(
  variants: readonly JsonSchemaObject[]
): Record<string, unknown> {
  const grouped = new Map<string, JsonSchemaObject[]>();
  for (const variant of variants) {
    if (!isRecord(variant.properties)) continue;
    for (const [name, raw] of Object.entries(variant.properties)) {
      if (!isJsonSchemaObject(raw)) continue;
      const schemas = grouped.get(name) ?? [];
      schemas.push(raw);
      grouped.set(name, schemas);
    }
  }

  return Object.fromEntries(
    [...grouped].map(([name, schemas]) => {
      const constants = schemas
        .filter(schema => 'const' in schema)
        .map(schema => schema.const);
      if (constants.length === schemas.length) {
        return [
          name,
          {
            enum: [...new Set(constants)],
            description:
              name === 'operation'
                ? 'Required operation selector.'
                : schemas[0]?.description,
          },
        ];
      }
      const distinctTypes = new Set(schemas.map(describeSchemaType));
      if (distinctTypes.size > 1) {
        return [name, { anyOf: schemas }];
      }
      return [name, schemas[0] ?? {}];
    })
  );
}

function intersectRequiredFields(
  variants: readonly JsonSchemaObject[],
  properties: Record<string, unknown>
): Set<string> {
  if (variants.length === 0) return new Set();
  const requiredByVariant = variants.map(
    variant => new Set(Array.isArray(variant.required) ? variant.required : [])
  );
  const common = [...(requiredByVariant[0] ?? new Set<string>())].filter(name =>
    requiredByVariant.every(required => required.has(name))
  );
  return new Set(
    common.filter(
      name =>
        !DIRECT_TOOL_AUTO_FILLED_FIELDS.has(name) &&
        !hasSchemaDefault(properties[name])
    )
  );
}

export function collectDisplayFields(
  properties: Record<string, unknown>,
  requiredFields: ReadonlySet<string>,
  prefix = ''
): DirectToolDisplayField[] {
  const fields: DirectToolDisplayField[] = [];

  for (const [name, value] of Object.entries(properties)) {
    if (!prefix && DIRECT_TOOL_AUTO_FILLED_FIELDS.has(name)) {
      continue;
    }

    const schema = isJsonSchemaObject(value) ? value : {};
    const fieldName = prefix ? `${prefix}.${name}` : name;
    fields.push({
      name: fieldName,
      required: requiredFields.has(name),
      type: describeSchemaType(schema),
      constraints: describeSchemaConstraints(schema),
      description:
        typeof schema.description === 'string' ? schema.description : undefined,
    });

    if (isRecord(schema.properties)) {
      const nestedRequired = new Set(
        Array.isArray(schema.required)
          ? schema.required.filter(nestedName =>
              typeof nestedName === 'string'
                ? !hasSchemaDefault(schema.properties?.[nestedName])
                : false
            )
          : []
      );
      fields.push(
        ...collectDisplayFields(schema.properties, nestedRequired, fieldName)
      );
    }

    const itemSchema =
      schema.type === 'array' && isJsonSchemaObject(schema.items)
        ? schema.items
        : undefined;
    if (itemSchema && isRecord(itemSchema.properties)) {
      const nestedRequired = new Set(
        Array.isArray(itemSchema.required)
          ? itemSchema.required.filter(nestedName =>
              typeof nestedName === 'string'
                ? !hasSchemaDefault(itemSchema.properties?.[nestedName])
                : false
            )
          : []
      );
      fields.push(
        ...collectDisplayFields(
          itemSchema.properties,
          nestedRequired,
          fieldName
        )
      );
    }
  }

  return fields;
}

export function buildExampleValue(name: string, type: string): unknown {
  if (type.startsWith('array<')) {
    const innerType = type.slice('array<'.length, -1);
    return [buildScalarExampleValue(name, innerType)];
  }

  return buildScalarExampleValue(name, type);
}

export function buildScalarExampleValue(name: string, type: string): unknown {
  if (type.startsWith('enum(')) {
    const match = /^enum\(([^,)]+)/.exec(type);
    return match?.[1] ?? name;
  }

  if (type === 'integer' || type === 'number') {
    return name === 'lineHint' ? 42 : 5;
  }

  if (type === 'boolean') {
    return true;
  }

  switch (name) {
    case 'keywords':
    case 'keywordsToSearch':
    case 'query':
    case 'text':
      return 'runCLI';
    case 'path':
      return '.';
    case 'uri':
      return '/path/to/file.ts';
    case 'owner':
      return 'bgauryy';
    case 'repo':
      return 'octocode';
    case 'extension':
      return 'ts';
    case 'filename':
      return 'package.json';
    case 'language':
      return 'TypeScript';
    case 'symbolName':
      return 'myFunction';
    case 'name':
    case 'packageName':
      return 'zod';
    default:
      return name;
  }
}

export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

export function isJsonSchemaObject(value: unknown): value is JsonSchemaObject {
  return isRecord(value);
}

export function hasSchemaDefault(value: unknown): boolean {
  return isJsonSchemaObject(value) && 'default' in value;
}
