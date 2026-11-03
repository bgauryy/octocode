// The shape of the config contract, once, in Zod. It validates the declaration, types every
// renderer, and is emitted as config-contract.schema.json for editors and external tools.
import { z } from 'zod';

const envName = z.string().regex(/^[A-Z][A-Z0-9_]*$/);
const typeName = z.string().regex(/^[A-Z][A-Za-z0-9]*$/);

// A non-empty array of distinct strings; `uniqueItems` reaches the emitted JSON Schema too.
const uniqueStrings = (item: z.ZodString) =>
  z
    .array(item)
    .min(1)
    .refine(items => new Set(items).size === items.length, 'items must be unique')
    .meta({ uniqueItems: true });

const dotenvPolicy = z.enum(['all', 'home', 'never']);

const environmentOnly = z.strictObject({
  dotenv: dotenvPolicy.optional(),
  tokenPriority: z.int().min(0).optional(),
  configSource: z.boolean().optional(),
  credential: z.boolean().optional(),
  documentation: z.boolean().optional(),
  description: z.string().optional(),
});

const envBinding = z.strictObject({
  name: envName,
  dotenv: dotenvPolicy.optional(),
  normalize: z.enum(['trim', 'lower']).optional(),
  workspaceNarrowsTo: z.string().min(1).optional(),
});

// Properties every field kind may carry; each kind below picks the ones it allows.
const common = {
  description: z.string().min(1),
  notes: z.string().optional(),
  env: envBinding.optional(),
  displayDefault: z.string().optional(),
};

const booleanField = z.strictObject({
  type: z.literal('boolean'),
  default: z.boolean(),
  ...common,
});

const stringField = z.strictObject({
  type: z.enum(['string', 'url', 'path']),
  default: z.string().nullable(),
  credential: z.boolean().optional(),
  ...common,
});

const stringArrayField = z.strictObject({
  type: z.literal('stringArray'),
  default: z.array(z.string()).nullable(),
  itemFormat: z.literal('path').optional(),
  ...common,
});

const enumField = z.strictObject({
  type: z.literal('enum'),
  values: uniqueStrings(z.string().min(1)),
  typeName: typeName.optional(),
  enumStyle: z.enum(['list', 'quotedOr']).optional(),
  ...common,
});

const field = z.discriminatedUnion('type', [booleanField, stringField, stringArrayField, enumField]);

const section = z.strictObject({
  title: z.string().min(1),
  typeName: typeName.optional(),
  rustTypeName: typeName.optional(),
  file: z.boolean(),
  resolved: z.boolean(),
  fields: z.record(z.string().regex(/^[a-z$][A-Za-z0-9]*$/), field),
});

export const contractSchema = z.strictObject({
  $schema: z.string(),
  config: z.strictObject({
    fileName: z.string().min(1),
    schemaVersion: z.int().min(1),
    runtimeSurfaces: uniqueStrings(z.string().regex(/^[a-z][a-z0-9]*$/)),
  }),
  environment: z.record(envName, environmentOnly),
  sections: z.record(z.string().regex(/^(|[a-z][A-Za-z0-9]*(\.[a-z][A-Za-z0-9]*)*)$/), section),
});

export type ContractInput = z.input<typeof contractSchema>;
type Parsed = z.output<typeof contractSchema>;

// Every field kind with the keys of the other kinds typed `undefined`, so renderers read any
// property without narrowing on `type` first.
type WithAbsentKeys<Kind, Key extends PropertyKey = Kind extends unknown ? keyof Kind : never> =
  Kind extends unknown ? Kind & { [Absent in Exclude<Key, keyof Kind>]?: undefined } : never;
export type Field = WithAbsentKeys<Parsed['sections'][string]['fields'][string]>;
export type Section = Omit<Parsed['sections'][string], 'fields'> & { fields: Record<string, Field> };
export type Contract = Omit<Parsed, 'sections'> & { sections: Record<string, Section> };

export const CONTRACT_SCHEMA_TITLE = 'Octocode declarative config contract';
export const CONTRACT_SCHEMA_ID = 'https://octocode.ai/schemas/config-contract.schema.json';
