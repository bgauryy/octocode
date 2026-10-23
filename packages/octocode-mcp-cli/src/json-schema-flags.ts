// Flags from an MCP tool's JSON Schema: one flag per property of an object root, typed
// by the property's `type` or `enum`. A keyword no flag can hold keeps the property as
// one partial JSON flag and leaves a fidelity note.
import { allocateFlag, notesOf } from './flag-names.js';
import type { Flag, ItemKind, JsonSchema } from './spec.js';

type FlagBase = Omit<Flag, 'kind'>;

function schemaNotes(schema: JsonSchema): string[] {
  const notes: string[] = [];
  for (const keyword of ['oneOf', 'anyOf', 'allOf', '$ref', 'patternProperties'] as const) {
    if (schema[keyword] !== undefined) notes.push(keyword);
  }
  if (schema.additionalProperties !== undefined && schema.additionalProperties !== false) {
    notes.push('additionalProperties');
  }
  return notes;
}

function isPrimitiveType(value: unknown): value is ItemKind {
  return value === 'string' || value === 'integer' || value === 'number' || value === 'boolean';
}

function isPropertyMap(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === 'object' && !Array.isArray(value);
}

/** The item kind of an array whose items are one plain primitive type. */
function arrayItemKind(items: unknown): ItemKind | undefined {
  if (!isPropertyMap(items)) return undefined;
  const itemType = items.type;
  return isPrimitiveType(itemType) && schemaNotes(items).length === 0 ? itemType : undefined;
}

/** The flag of a property whose schema uses no keyword a flag cannot hold. */
function typedFlag(base: FlagBase, record: JsonSchema, notes: string[]): { flag: Flag; notes: string[] } {
  const enumValues = Array.isArray(record.enum) ? record.enum : undefined;
  if (enumValues && enumValues.every(value => typeof value !== 'object')) {
    return { flag: { ...base, kind: 'enum', enumValues }, notes };
  }
  const type = record.type;
  if (Array.isArray(type)) {
    return { flag: { ...base, kind: 'json', partial: true }, notes: ['union'] };
  }
  if (type === 'string' || type === 'integer' || type === 'number') return { flag: { ...base, kind: type }, notes };
  if (type === 'boolean') {
    const presence = !base.required && base.hasDefault && base.defaultValue === false;
    return { flag: { ...base, kind: 'boolean', presence }, notes };
  }
  if (type === 'array') {
    const itemKind = arrayItemKind(record.items);
    if (itemKind) return { flag: { ...base, kind: 'array', itemKind }, notes };
    return { flag: { ...base, kind: 'json', partial: true }, notes: ['array'] };
  }
  return { flag: { ...base, kind: 'json', partial: true }, notes: notesOf(notes, ['partial']) };
}

function flagFromJsonProperty(
  property: string,
  schema: unknown,
  required: boolean,
  name: string,
): { flag: Flag; notes: string[] } {
  if (!isPropertyMap(schema)) {
    return {
      flag: {
        property,
        name,
        kind: 'json',
        required,
        presence: false,
        partial: true,
        hasDefault: false,
      },
      notes: ['partial'],
    };
  }
  const notes = schemaNotes(schema);
  const hasDefault = Object.prototype.hasOwnProperty.call(schema, 'default');
  const defaultValue = schema.default;
  // A title is the property's label (JSON Schema 2020-12 §9.1); help shows it when no description exists.
  const description = typeof schema.description === 'string' ? schema.description : typeof schema.title === 'string' ? schema.title : undefined;
  const base = {
    property,
    name,
    required: hasDefault ? false : required,
    presence: false,
    partial: false,
    hasDefault,
    ...(description ? { description } : {}),
    ...(hasDefault ? { defaultValue } : {}),
  };
  if (notes.length > 0) {
    return { flag: { ...base, kind: 'json', partial: true }, notes };
  }
  return typedFlag(base, schema, notes);
}

/** The properties a root schema takes as flags, or undefined when the root is not an object. */
export function objectProperties(schema: JsonSchema): Record<string, unknown> | undefined {
  const properties = schema.properties;
  if (schema.type === 'object') {
    if (properties === undefined) return {};
    return isPropertyMap(properties) ? properties : undefined;
  }
  if (schema.type !== undefined) return undefined;
  if (isPropertyMap(properties)) return properties;
  const known = new Set(['$schema', 'additionalProperties', 'description', 'title', 'required']);
  const extra = Object.keys(schema).filter(key => !known.has(key));
  return extra.length === 0 ? {} : undefined;
}

export function flagsFromJsonSchema(schema: JsonSchema): { flags: Flag[]; notes: string[] } {
  const rootNotes = schemaNotes(schema);
  const properties = objectProperties(schema);
  if (!properties) {
    return {
      flags: [
        {
          property: 'input',
          name: 'input',
          kind: 'json',
          required: true,
          presence: false,
          partial: true,
          hasDefault: false,
        },
      ],
      notes: notesOf(rootNotes, ['partial']),
    };
  }
  const required = new Set(Array.isArray(schema.required) ? schema.required.map(String) : []);
  const used = new Set<string>();
  const flags: Flag[] = [];
  const notes = [...rootNotes];
  for (const [property, child] of Object.entries(properties)) {
    const built = flagFromJsonProperty(property, child, required.has(property), allocateFlag(property, used));
    flags.push(built.flag);
    notes.push(...built.notes);
  }
  if (flags.some(flag => flag.partial)) notes.push('partial');
  return { flags, notes: notesOf(notes) };
}
