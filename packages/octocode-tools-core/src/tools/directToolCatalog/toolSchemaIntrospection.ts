/**
 * JSON-schema introspection for the engine-free direct-tool catalog: display
 * field extraction, constraint/type text formatting, and example-value
 * builders. Split out of `directToolCatalog.meta.ts` (still the public
 * barrel) — see that file's header comment for the full P3 rationale.
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

  const properties = isRecord(jsonSchema.properties)
    ? jsonSchema.properties
    : {};

  const requiredFields = new Set(
    Array.isArray(jsonSchema.required)
      ? jsonSchema.required.filter(
          name =>
            !DIRECT_TOOL_AUTO_FILLED_FIELDS.has(name) &&
            !hasSchemaDefault(properties[name])
        )
      : []
  );

  return collectDisplayFields(properties, requiredFields);
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
