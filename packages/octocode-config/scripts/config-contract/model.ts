// The validated config contract and the views both renderers (TypeScript source and the
// settings reference) derive from it: flattened fields, defaults, and environment policy.
import Ajv2020 from 'ajv/dist/2020.js';
import { readFile } from 'node:fs/promises';

export type JsonObject = Record<string, unknown>;
type EnvBinding = {
  name: string;
  dotenv?: 'all' | 'home' | 'never';
  normalize?: 'trim' | 'lower';
  workspaceNarrowsTo?: string;
};
export type Field = {
  type: 'boolean' | 'string' | 'url' | 'path' | 'stringArray' | 'enum';
  description: string;
  notes?: string;
  env?: EnvBinding;
  default?: unknown;
  credential?: boolean;
  displayDefault?: string;
  itemFormat?: 'path';
  typeName?: string;
  enumStyle?: 'list' | 'quotedOr';
  values?: string[];
};
export type Section = {
  title: string;
  typeName?: string;
  rustTypeName?: string;
  file: boolean;
  resolved: boolean;
  fields: Record<string, Field>;
};
export type Contract = {
  config: {
    fileName: string;
    schemaVersion: number;
    runtimeSurfaces: string[];
  };
  environment: Record<
    string,
    {
      dotenv?: 'all' | 'home' | 'never';
      tokenPriority?: number;
      configSource?: boolean;
      documentation?: boolean;
      description?: string;
    }
  >;
  removed?: {
    settings?: Record<string, string>;
    environment?: Record<string, string>;
  };
  sections: Record<string, Section>;
};

export type FlatField = Field & {
  path: string;
  section: string;
  key: string;
  file: boolean;
  resolved: boolean;
};

function isJsonObject(value: unknown): value is JsonObject {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function asObject(value: unknown, label: string): JsonObject {
  if (!isJsonObject(value)) {
    throw new Error(`${label} must be an object`);
  }
  return value;
}

function pathFor(section: string, key: string): string {
  return section.length === 0 ? key : `${section}.${key}`;
}

export function flattenFields(contract: Contract): FlatField[] {
  return Object.entries(contract.sections).flatMap(([sectionPath, section]) =>
    Object.entries(section.fields).map(([key, field]) => ({
      ...field,
      path: pathFor(sectionPath, key),
      section: sectionPath,
      key,
      file: section.file,
      resolved: section.resolved && field.credential !== true,
    }))
  );
}

export function setPath(root: JsonObject, path: string, value: unknown): void {
  const parts = path.split('.');
  let current = root;
  for (const part of parts.slice(0, -1)) {
    const existing = current[part];
    if (isJsonObject(existing)) {
      current = existing;
    } else {
      const child: JsonObject = {};
      current[part] = child;
      current = child;
    }
  }
  current[parts.at(-1)!] = value;
}

export function ownDefault(field: FlatField): unknown {
  return field.type === 'enum' ? field.values![0] : structuredClone(field.default);
}

export function orderedEnvironment(fields: FlatField[], contract: Contract) {
  const fieldBindings = fields.flatMap(field =>
    field.env ? [{ field: field.path, ...field.env }] : []
  );
  const protectedNames: string[] = [];
  const homeTrusted: string[] = [];
  const workspaceNarrowOnly: Record<string, string> = {};
  for (const [name, definition] of Object.entries(contract.environment)) {
    if ((definition.dotenv ?? 'all') !== 'all') protectedNames.push(name);
    if (definition.dotenv === 'home') homeTrusted.push(name);
  }
  for (const binding of fieldBindings) {
    if ((binding.dotenv ?? 'all') !== 'all') protectedNames.push(binding.name);
    if (binding.dotenv === 'home') homeTrusted.push(binding.name);
    if (binding.workspaceNarrowsTo) workspaceNarrowOnly[binding.name] = binding.workspaceNarrowsTo;
  }
  const tokens = Object.entries(contract.environment).filter(
    ([, definition]) => definition.tokenPriority !== undefined
  );
  tokens.sort(([, left], [, right]) => left.tokenPriority! - right.tokenPriority!);
  const tokenNames = tokens.map(([name]) => name);
  return {
    fieldBindings,
    protectedNames: [...new Set(protectedNames)],
    homeTrusted: [...new Set(homeTrusted)],
    tokenNames,
    workspaceNarrowOnly,
  };
}

export async function loadContract(contractPath: string, schemaPath: string): Promise<Contract> {
  const [contractSource, schemaSource] = await Promise.all([
    readFile(contractPath, 'utf8'),
    readFile(schemaPath, 'utf8'),
  ]);
  const contract: unknown = JSON.parse(contractSource);
  const schema = asObject(JSON.parse(schemaSource) as unknown, 'config contract schema');
  const ajv = new Ajv2020({ allErrors: true, strict: false });
  const validate = ajv.compile<Contract>(schema);
  if (!validate(contract)) {
    throw new Error(`Invalid config contract:\n${ajv.errorsText(validate.errors, { separator: '\n' })}`);
  }
  asObject(contract, 'config contract');
  return contract;
}
