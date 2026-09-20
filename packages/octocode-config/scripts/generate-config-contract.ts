import Ajv2020 from 'ajv/dist/2020.js';
import { readFile, writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';

const contractPath = fileURLToPath(new URL('../config-contract.json', import.meta.url));
const schemaPath = fileURLToPath(
  new URL('../config-contract.schema.json', import.meta.url)
);
const outputPath = fileURLToPath(
  new URL('../src/config/contract.generated.ts', import.meta.url)
);

type JsonObject = Record<string, unknown>;
type EnvBinding = {
  priority: number;
  dotenv?: 'all' | 'home' | 'never';
  normalize?: 'trim' | 'lower';
  invalid?: 'skip' | 'default';
};
type Field = {
  type: 'boolean' | 'number' | 'string' | 'url' | 'path' | 'stringArray' | 'enum' | 'schemaVersion';
  description: string;
  notes?: string;
  env?: Record<string, EnvBinding>;
  default?: unknown;
  defaultFrom?: string;
  credential?: boolean;
  displayDefault?: string;
  itemFormat?: 'path';
  typeName?: string;
  constantName?: string;
  boundsName?: string;
  valuesConstant?: string;
  enumStyle?: 'list' | 'quotedOr';
  minimum?: number;
  span?: number;
  defaultOffset?: number;
  values?: string[];
};
type Section = {
  title: string;
  typeName?: string;
  file: boolean;
  resolved: boolean;
  fields: Record<string, Field>;
};
type Contract = {
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
      description?: string;
    }
  >;
  sections: Record<string, Section>;
};

type FlatField = Field & {
  path: string;
  section: string;
  key: string;
  file: boolean;
  resolved: boolean;
};

function asObject(value: unknown, label: string): JsonObject {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    throw new Error(`${label} must be an object`);
  }
  return value as JsonObject;
}

function toScreamingSnake(value: string): string {
  return value
    .replace(/\./g, '_')
    .replace(/([A-Z]+)([A-Z][a-z])/g, '$1_$2')
    .replace(/([a-z0-9])([A-Z])/g, '$1_$2')
    .toUpperCase();
}

function toPascal(value: string): string {
  return value
    .split('.')
    .flatMap(part => part.split(/(?=[A-Z])/))
    .map(part => part.charAt(0).toUpperCase() + part.slice(1))
    .join('');
}

function pathFor(section: string, key: string): string {
  return section.length === 0 ? key : `${section}.${key}`;
}

function flattenFields(contract: Contract): FlatField[] {
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

function setPath(root: JsonObject, path: string, value: unknown): void {
  const parts = path.split('.');
  let current = root;
  for (const part of parts.slice(0, -1)) {
    const existing = current[part];
    if (typeof existing === 'object' && existing !== null && !Array.isArray(existing)) {
      current = existing as JsonObject;
    } else {
      const child: JsonObject = {};
      current[part] = child;
      current = child;
    }
  }
  current[parts.at(-1)!] = value;
}

function getPath(root: JsonObject, path: string): unknown {
  let current: unknown = root;
  for (const part of path.split('.')) {
    if (typeof current !== 'object' || current === null || Array.isArray(current)) {
      return undefined;
    }
    current = (current as JsonObject)[part];
  }
  return current;
}

function ownDefault(field: FlatField, schemaVersion: number): unknown {
  switch (field.type) {
    case 'schemaVersion':
      return schemaVersion;
    case 'number': {
      const minimum = field.minimum!;
      const maximum = minimum + field.span!;
      return minimum + Math.min(field.defaultOffset!, maximum - minimum);
    }
    case 'enum':
      return field.values![0];
    default:
      return structuredClone(field.default);
  }
}

function buildDefaults(contract: Contract, fields: FlatField[]): JsonObject {
  const result: JsonObject = {};
  const pending = fields.filter(field => field.resolved);
  while (pending.length > 0) {
    let progress = false;
    for (let index = pending.length - 1; index >= 0; index--) {
      const field = pending[index]!;
      if (field.defaultFrom) {
        const inherited = getPath(result, field.defaultFrom);
        if (inherited === undefined) continue;
        setPath(result, field.path, structuredClone(inherited));
      } else {
        setPath(result, field.path, ownDefault(field, contract.config.schemaVersion));
      }
      pending.splice(index, 1);
      progress = true;
    }
    if (!progress) {
      throw new Error(
        `Unresolved or cyclic defaultFrom paths: ${pending.map(field => field.path).join(', ')}`
      );
    }
  }
  return result;
}

function directChildren(sectionPath: string, sections: Record<string, Section>): string[] {
  const prefix = sectionPath.length === 0 ? '' : `${sectionPath}.`;
  return Object.keys(sections).filter(candidate => {
    if (candidate === '' || !candidate.startsWith(prefix) || candidate === sectionPath) {
      return false;
    }
    return !candidate.slice(prefix.length).includes('.');
  });
}

function sectionInterfaceName(
  sectionPath: string,
  required: boolean,
  sections: Record<string, Section>
): string {
  const base = `${sections[sectionPath]?.typeName ?? toPascal(sectionPath)}Config`;
  return required ? `Required${base}` : `${base}Options`;
}

function enumType(field: Field): string {
  if (field.typeName) return field.typeName;
  return field.values!.map(value => JSON.stringify(value)).join(' | ');
}

function inputFieldType(field: Field): string {
  switch (field.type) {
    case 'schemaVersion':
    case 'number':
      return 'number';
    case 'boolean':
      return 'boolean';
    case 'stringArray':
      return field.default === null ? 'string[] | null' : 'string[]';
    case 'enum':
      return enumType(field);
    case 'string':
    case 'url':
    case 'path':
      return field.credential ? 'string | null' : 'string';
  }
}

function resolvedFieldType(field: Field): string {
  switch (field.type) {
    case 'schemaVersion':
    case 'number':
      return 'number';
    case 'boolean':
      return 'boolean';
    case 'stringArray':
      return field.default === null ? 'string[] | null' : 'string[]';
    case 'enum':
      return enumType(field);
    case 'string':
    case 'url':
    case 'path':
      return field.default === null ? 'string | undefined' : 'string';
  }
}

function renderFieldDocs(field: Field, indent = '  '): string {
  const lines = [`${indent}/** ${field.description.replace(/\*\//g, '* /')} */`];
  if (field.notes) lines.push(`${indent}/** ${field.notes.replace(/\*\//g, '* /')} */`);
  return lines.join('\n');
}

function renderInterfaces(contract: Contract): string {
  const sections = contract.sections;
  const enumAliases = new Map<string, string>();
  for (const section of Object.values(sections)) {
    for (const field of Object.values(section.fields)) {
      if (field.type === 'enum' && field.typeName) {
        enumAliases.set(field.typeName, field.values!.map(value => JSON.stringify(value)).join(' | '));
      }
    }
  }

  const inputInterfaces = Object.entries(sections)
    .filter(([path, section]) => path !== '' && section.file)
    .map(([sectionPath, section]) => {
      const members = Object.entries(section.fields).map(
        ([key, field]) => `${renderFieldDocs(field)}\n  ${key}?: ${inputFieldType(field)};`
      );
      for (const child of directChildren(sectionPath, sections).filter(path => sections[path]!.file)) {
        const key = child.slice(sectionPath.length + 1);
        members.push(`  ${key}?: ${sectionInterfaceName(child, false, sections)};`);
      }
      return `export interface ${sectionInterfaceName(sectionPath, false, sections)} {\n${members.join('\n\n')}\n}`;
    })
    .join('\n\n');

  const root = sections['']!;
  const rootMembers = [
    '  $schema?: string;',
    ...Object.entries(root.fields).map(
      ([key, field]) => `${renderFieldDocs(field)}\n  ${key}?: ${inputFieldType(field)};`
    ),
    ...directChildren('', sections)
      .filter(path => sections[path]!.file)
      .map(path => `  ${path}?: ${sectionInterfaceName(path, false, sections)};`),
  ];

  const requiredInterfaces = Object.entries(sections)
    .filter(([path, section]) => path !== '' && section.resolved)
    .map(([sectionPath, section]) => {
      const members = Object.entries(section.fields)
        .filter(([, field]) => !field.credential)
        .map(([key, field]) => `  ${key}: ${resolvedFieldType(field)};`);
      for (const child of directChildren(sectionPath, sections).filter(path => sections[path]!.resolved)) {
        const key = child.slice(sectionPath.length + 1);
        members.push(`  ${key}: ${sectionInterfaceName(child, true, sections)};`);
      }
      return `export interface ${sectionInterfaceName(sectionPath, true, sections)} {\n${members.join('\n')}\n}`;
    })
    .join('\n\n');

  const resolvedRootMembers = [
    ...Object.entries(root.fields)
      .filter(([, field]) => !field.credential)
      .map(([key, field]) => `  ${key}: ${resolvedFieldType(field)};`),
    ...directChildren('', sections)
      .filter(path => sections[path]!.resolved)
      .map(path => `  ${path}: ${sectionInterfaceName(path, true, sections)};`),
  ];

  return `${[...enumAliases].map(([name, type]) => `export type ${name} = ${type};`).join('\n')}

${inputInterfaces}

export interface OctocodeConfig {
${rootMembers.join('\n\n')}
}

${requiredInterfaces}

export interface ResolvedConfigData {
${resolvedRootMembers.join('\n')}
}`;
}

function renderTsValue(
  value: unknown,
  path: string,
  fieldByPath: Map<string, FlatField>
): string {
  if (value === null) {
    const field = fieldByPath.get(path);
    if (field && ['string', 'url', 'path'].includes(field.type)) return 'undefined';
    return 'null';
  }
  if (Array.isArray(value)) return JSON.stringify(value);
  if (typeof value !== 'object') return JSON.stringify(value);
  const entries = Object.entries(value as JsonObject).map(
    ([key, child]) => `${JSON.stringify(key)}: ${renderTsValue(child, path ? `${path}.${key}` : key, fieldByPath)}`
  );
  return `{ ${entries.join(', ')} }`;
}

function orderedEnvironment(fields: FlatField[], contract: Contract) {
  const fieldBindings = fields.flatMap(field =>
    Object.entries(field.env ?? {})
      .sort(([, left], [, right]) => left.priority - right.priority)
      .map(([name, binding]) => ({ name, field: field.path, ...binding }))
  );
  const protectedNames: string[] = [];
  const homeTrusted: string[] = [];
  const sourceNames: string[] = [];
  for (const [name, definition] of Object.entries(contract.environment)) {
    if ((definition.dotenv ?? 'all') !== 'all') protectedNames.push(name);
    if (definition.dotenv === 'home') homeTrusted.push(name);
    if (definition.configSource) sourceNames.push(name);
  }
  for (const binding of fieldBindings) {
    if ((binding.dotenv ?? 'all') !== 'all') protectedNames.push(binding.name);
    if (binding.dotenv === 'home') homeTrusted.push(binding.name);
    sourceNames.push(binding.name);
  }
  const tokenNames = Object.entries(contract.environment)
    .filter(([, definition]) => definition.tokenPriority !== undefined)
    .sort(([, left], [, right]) => left.tokenPriority! - right.tokenPriority!)
    .map(([name]) => name);
  return {
    fieldBindings,
    protectedNames: [...new Set(protectedNames)],
    homeTrusted: [...new Set(homeTrusted)],
    sourceNames: [...new Set(sourceNames)],
    tokenNames,
  };
}

function renderConstants(fields: FlatField[], defaults: JsonObject): string {
  const output: string[] = [];
  for (const field of fields) {
    if (!field.resolved || field.type === 'schemaVersion' || field.defaultFrom) continue;
    const constantName = field.constantName ?? toScreamingSnake(field.path);
    const value = getPath(defaults, field.path);
    const constAssertion = value === null ? '' : ' as const';
    output.push(`export const DEFAULT_${constantName} = ${JSON.stringify(value)}${constAssertion};`);
    if (field.type === 'number') {
      const boundsName = field.boundsName ?? constantName;
      output.push(`export const MIN_${boundsName} = ${field.minimum};`);
      output.push(`export const MAX_${boundsName} = ${field.minimum! + field.span!};`);
    }
    if (field.type === 'enum' && field.valuesConstant) {
      output.push(`export const ${field.valuesConstant} = ${JSON.stringify(field.values)} as const;`);
    }
  }
  return output.join('\n');
}

function render(contract: Contract): string {
  const fields = flattenFields(contract);
  const defaults = buildDefaults(contract, fields);
  const fieldByPath = new Map(fields.map(field => [field.path, field]));
  const environment = orderedEnvironment(fields, contract);
  const metadata = fields.map(field => ({
    path: field.path,
    section: field.section,
    key: field.key,
    type: field.type,
    file: field.file,
    resolved: field.resolved,
    credential: field.credential ?? false,
    description: field.description,
    notes: field.notes,
    env: Object.entries(field.env ?? {})
      .sort(([, left], [, right]) => left.priority - right.priority)
      .map(([name, binding]) => ({ name, ...binding })),
    defaultValue: field.defaultFrom ? getPath(defaults, field.path) : ownDefault(field, contract.config.schemaVersion),
    defaultFrom: field.defaultFrom,
    minimum: field.minimum,
    maximum: field.minimum !== undefined ? field.minimum + field.span! : undefined,
    values: field.values,
    enumStyle: field.enumStyle,
    itemFormat: field.itemFormat,
  }));

  return `// @generated by scripts/generate-config-contract.ts from config-contract.json.
// Do not edit; run \`yarn workspace @octocodeai/config generate:config-contract\`.

${renderInterfaces(contract)}

export type ConfigFieldKind = 'boolean' | 'number' | 'string' | 'url' | 'path' | 'stringArray' | 'enum' | 'schemaVersion';
export interface ConfigEnvBinding {
  name: string;
  priority: number;
  dotenv?: 'all' | 'home' | 'never';
  normalize?: 'trim' | 'lower';
  invalid?: 'skip' | 'default';
}
export interface ConfigFieldSpec {
  path: string;
  section: string;
  key: string;
  type: ConfigFieldKind;
  file: boolean;
  resolved: boolean;
  credential: boolean;
  description: string;
  notes?: string;
  env: readonly ConfigEnvBinding[];
  defaultValue: unknown;
  defaultFrom?: string;
  minimum?: number;
  maximum?: number;
  values?: readonly string[];
  enumStyle?: 'list' | 'quotedOr';
  itemFormat?: 'path';
}

export const CONFIG_FIELDS: readonly ConfigFieldSpec[] = ${JSON.stringify(metadata, null, 2)};
export const CONFIG_SCHEMA_VERSION = ${contract.config.schemaVersion};
export const CONFIG_FILE_NAME = ${JSON.stringify(contract.config.fileName)};
export const RUNTIME_SURFACES = ${JSON.stringify(contract.config.runtimeSurfaces)} as const;
export type RuntimeSurface = (typeof RUNTIME_SURFACES)[number];
export const DEFAULT_RUNTIME_SURFACE: RuntimeSurface = RUNTIME_SURFACES[0];
export const ENV_TOKEN_VARS = ${JSON.stringify(environment.tokenNames)} as const;
export type EnvTokenVar = (typeof ENV_TOKEN_VARS)[number];
export const PROTECTED_KEY_NAMES = ${JSON.stringify(environment.protectedNames)} as const;
export const HOME_TRUSTED_ENV_KEYS = ${JSON.stringify(environment.homeTrusted)} as const;
export const CONFIG_SOURCE_ENV_KEYS = ${JSON.stringify(environment.sourceNames)} as const;
export type ConfigSourceEnvKey = (typeof CONFIG_SOURCE_ENV_KEYS)[number];
export const DEFAULT_CONFIG_VALUE: ResolvedConfigData = ${renderTsValue(defaults, '', fieldByPath)};
${renderConstants(fields, defaults)}
`;
}

async function loadContract(): Promise<Contract> {
  const [contractSource, schemaSource] = await Promise.all([
    readFile(contractPath, 'utf8'),
    readFile(schemaPath, 'utf8'),
  ]);
  const contract = JSON.parse(contractSource) as unknown;
  const schema = asObject(JSON.parse(schemaSource) as unknown, 'config contract schema');
  const ajv = new Ajv2020({ allErrors: true, strict: false });
  const validate = ajv.compile(schema);
  if (!validate(contract)) {
    throw new Error(`Invalid config contract:\n${ajv.errorsText(validate.errors, { separator: '\n' })}`);
  }
  asObject(contract, 'config contract');
  return contract as Contract;
}

export async function generateConfigContract(): Promise<string> {
  return render(await loadContract());
}

async function main(): Promise<void> {
  const output = await generateConfigContract();
  if (process.argv.includes('--check')) {
    const current = await readFile(outputPath, 'utf8').catch(() => '');
    if (current !== output) {
      throw new Error(
        'Generated config contract is stale. Run: yarn workspace @octocodeai/config generate:config-contract'
      );
    }
    return;
  }
  await writeFile(outputPath, output);
}

await main();
