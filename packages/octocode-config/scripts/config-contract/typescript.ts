// Renders src/config/contract.generated.ts: resolved-config interfaces, field metadata,
// environment policy, and the default config value.
import {
  type Contract,
  type FlatField,
  type JsonObject,
  type Field,
  type Section,
  flattenFields,
  orderedEnvironment,
  ownDefault,
  setPath,
} from './model.ts';

function toPascal(value: string): string {
  return value
    .split('.')
    .flatMap(part => part.split(/(?=[A-Z])/))
    .map(part => part.charAt(0).toUpperCase() + part.slice(1))
    .join('');
}

function buildDefaults(fields: FlatField[]): JsonObject {
  const result: JsonObject = {};
  for (const field of fields.filter(candidate => candidate.resolved)) {
    setPath(result, field.path, ownDefault(field));
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

function sectionInterfaceName(sectionPath: string, sections: Record<string, Section>): string {
  return `Required${sections[sectionPath]?.typeName ?? toPascal(sectionPath)}Config`;
}

function enumType(field: Field): string {
  if (field.typeName) return field.typeName;
  return (field.values ?? []).map(value => JSON.stringify(value)).join(' | ');
}

function resolvedFieldType(field: Field): string {
  if (field.type === 'boolean') return 'boolean';
  if (field.type === 'stringArray') return field.default === null ? 'string[] | null' : 'string[]';
  if (field.type === 'enum') return enumType(field);
  return field.default === null ? 'string | undefined' : 'string';
}

function renderInterfaces(contract: Contract): string {
  const sections = contract.sections;
  const enumAliases = new Map<string, string>();
  for (const section of Object.values(sections)) {
    for (const field of Object.values(section.fields)) {
      if (field.type === 'enum' && field.typeName) {
        enumAliases.set(field.typeName, (field.values ?? []).map(value => JSON.stringify(value)).join(' | '));
      }
    }
  }

  const root = sections[''];
  if (!root) throw new Error('The config contract has no root section.');
  const requiredInterfaces = Object.entries(sections)
    .filter(([path, section]) => path !== '' && section.resolved)
    .map(([sectionPath, section]) => {
      const members = Object.entries(section.fields)
        .filter(([, field]) => !field.credential)
        .map(([key, field]) => `  ${key}: ${resolvedFieldType(field)};`);
      for (const child of directChildren(sectionPath, sections).filter(path => sections[path]?.resolved)) {
        const key = child.slice(sectionPath.length + 1);
        members.push(`  ${key}: ${sectionInterfaceName(child, sections)};`);
      }
      return `export interface ${sectionInterfaceName(sectionPath, sections)} {\n${members.join('\n')}\n}`;
    })
    .join('\n\n');

  const resolvedRootMembers = [
    ...Object.entries(root.fields)
      .filter(([, field]) => !field.credential)
      .map(([key, field]) => `  ${key}: ${resolvedFieldType(field)};`),
    ...directChildren('', sections)
      .filter(path => sections[path]?.resolved)
      .map(path => `  ${path}: ${sectionInterfaceName(path, sections)};`),
  ];

  return `${[...enumAliases].map(([name, type]) => `export type ${name} = ${type};`).join('\n')}

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
  const entries = Object.entries(value).map(
    ([key, child]) => `${JSON.stringify(key)}: ${renderTsValue(child, path ? `${path}.${key}` : key, fieldByPath)}`
  );
  return `{ ${entries.join(', ')} }`;
}

export function renderTypeScript(contract: Contract): string {
  const fields = flattenFields(contract);
  const defaults = buildDefaults(fields);
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
    env: field.env,
    defaultValue: ownDefault(field),
    values: field.values,
    enumStyle: field.enumStyle,
    itemFormat: field.itemFormat,
  }));

  return `// @generated by scripts/generate-config-contract.ts from config-contract.json.
// Do not edit; run \`yarn workspace @octocodeai/config generate:config-contract\`.

${renderInterfaces(contract)}

export type ConfigFieldKind = 'boolean' | 'string' | 'url' | 'path' | 'stringArray' | 'enum';
export interface ConfigEnvBinding {
  name: string;
  dotenv?: 'all' | 'home' | 'never';
  normalize?: 'trim' | 'lower';
  workspaceNarrowsTo?: string;
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
  /** The one variable that sets this field, if any. */
  env?: ConfigEnvBinding;
  defaultValue: unknown;
  values?: readonly string[];
  enumStyle?: 'list' | 'quotedOr';
  itemFormat?: 'path';
}

export const CONFIG_FIELDS: readonly ConfigFieldSpec[] = ${JSON.stringify(metadata, null, 2)};
export const RUNTIME_SURFACES = ${JSON.stringify(contract.config.runtimeSurfaces)} as const;
export type RuntimeSurface = (typeof RUNTIME_SURFACES)[number];
export const ENV_TOKEN_VARS = ${JSON.stringify(environment.tokenNames)} as const;
export const ENV_CREDENTIAL_VARS = ${JSON.stringify(environment.credentialNames)} as const;
export const PROTECTED_KEY_NAMES = ${JSON.stringify(environment.protectedNames)} as const;
export const HOME_TRUSTED_ENV_KEYS = ${JSON.stringify(environment.homeTrusted)} as const;
/** Home-trusted switches a workspace may set only to this narrowing value. */
export const WORKSPACE_NARROW_ONLY_ENV: Readonly<Record<string, string>> = ${JSON.stringify(environment.workspaceNarrowOnly)};
export const DEFAULT_CONFIG_VALUE: ResolvedConfigData = ${renderTsValue(defaults, '', fieldByPath)};
`;
}
