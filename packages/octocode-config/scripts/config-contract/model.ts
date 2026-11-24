// The validated config contract and the views both renderers (TypeScript source and the
// settings reference) derive from it: flattened fields, defaults, and environment policy.
import type { Contract, Field } from './schema.ts';

export type JsonObject = Record<string, unknown>;

export type FlatField = Field & {
  path: string;
  section: string;
  key: string;
  resolved: boolean;
};

function isJsonObject(value: unknown): value is JsonObject {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
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
      resolved: field.credential !== true,
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
  const last = parts.at(-1);
  if (last !== undefined) current[last] = value;
}

export function ownDefault(field: FlatField): unknown {
  return field.type === 'enum' ? field.values?.[0] : structuredClone(field.default);
}

export function orderedEnvironment(fields: FlatField[], contract: Contract) {
  const fieldBindings = fields.flatMap(field =>
    field.env ? [{ field: field.path, credential: field.credential === true, ...field.env }] : []
  );
  const protectedNames: string[] = [];
  const homeTrusted: string[] = [];
  const credentialNames = new Set<string>();
  const workspaceNarrowOnly: Record<string, string> = {};
  for (const [name, definition] of Object.entries(contract.environment)) {
    if ((definition.dotenv ?? 'all') !== 'all') protectedNames.push(name);
    if (definition.dotenv === 'home') homeTrusted.push(name);
    if (definition.credential === true) credentialNames.add(name);
  }
  for (const binding of fieldBindings) {
    if ((binding.dotenv ?? 'all') !== 'all') protectedNames.push(binding.name);
    if (binding.dotenv === 'home') homeTrusted.push(binding.name);
    if (binding.credential) credentialNames.add(binding.name);
    if (binding.workspaceNarrowsTo) workspaceNarrowOnly[binding.name] = binding.workspaceNarrowsTo;
  }
  // A legacy name carries its current name's policy, so either spelling is trusted, protected and
  // masked alike. Each maps to the current name it is read as, in declaration order.
  const legacyNames: Record<string, string> = {};
  for (const [name, definition] of Object.entries(contract.environment)) {
    for (const legacy of definition.legacyNames ?? []) legacyNames[legacy] = name;
  }
  for (const binding of fieldBindings) {
    for (const legacy of binding.legacyNames ?? []) legacyNames[legacy] = binding.name;
  }
  for (const [legacy, current] of Object.entries(legacyNames)) {
    if (protectedNames.includes(current)) protectedNames.push(legacy);
    if (homeTrusted.includes(current)) homeTrusted.push(legacy);
    if (credentialNames.has(current)) credentialNames.add(legacy);
    const narrowsTo = workspaceNarrowOnly[current];
    if (narrowsTo !== undefined) workspaceNarrowOnly[legacy] = narrowsTo;
  }
  const legacyPaths = Object.fromEntries(
    fields.flatMap(field => (field.legacyPaths ?? []).map(legacy => [legacy, field.path]))
  );
  const tokens = Object.entries(contract.environment).filter(
    ([, definition]) => definition.tokenPriority !== undefined
  );
  tokens.sort(([, left], [, right]) => (left.tokenPriority ?? 0) - (right.tokenPriority ?? 0));
  const tokenNames = tokens.map(([name]) => name);
  // Every API key a person sets: environment credentials of type key, then credential settings.
  const apiKeys = [
    ...Object.entries(contract.environment)
      .filter(([, definition]) => definition.type === 'key')
      .map(([name, definition]) => ({ name, dotenv: definition.dotenv ?? 'all', group: definition.group, description: definition.description, docsUrl: definition.docsUrl })),
    ...fields
      .filter(field => field.credential === true && field.env)
      .map(field => ({ name: field.env!.name, dotenv: field.env!.dotenv ?? 'all', group: field.group, description: field.description, docsUrl: field.docsUrl })),
  ].map(key => Object.fromEntries(Object.entries(key).filter(([, value]) => value !== undefined)));
  return {
    apiKeys,
    fieldBindings,
    protectedNames: [...new Set(protectedNames)],
    homeTrusted: [...new Set(homeTrusted)],
    credentialNames: [...credentialNames].toSorted(),
    tokenNames,
    workspaceNarrowOnly,
    legacyNames,
    legacyPaths,
  };
}
