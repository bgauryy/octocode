import path from 'node:path';
import {
  CONFIG_FIELDS,
  CONFIG_SOURCE_ENV_KEYS,
  DEFAULT_CONFIG_VALUE,
  type ConfigFieldSpec,
  type ConfigSourceEnvKey,
  type ResolvedConfigData,
} from './contract.generated.js';
import type {
  OctocodeConfig,
  RequiredGitHubConfig,
  RequiredLocalConfig,
  RequiredLspConfig,
  RequiredNetworkConfig,
  RequiredOutputConfig,
  RequiredStorageConfig,
  RequiredToolsConfig,
} from './types.js';

export { CONFIG_SOURCE_ENV_KEYS, type ConfigSourceEnvKey };

export function parseBooleanEnv(value: string | undefined): boolean | undefined {
  if (value === undefined) return undefined;
  switch (value.trim().toLowerCase()) {
    case 'true':
    case '1':
      return true;
    case 'false':
    case '0':
      return false;
    default:
      return undefined;
  }
}

export function parseIntEnv(value: string | undefined): number | undefined {
  if (value === undefined || value.trim() === '') return undefined;
  const parsed = Number.parseInt(value, 10);
  return Number.isNaN(parsed) ? undefined : parsed;
}

export function parseStringArrayEnv(value: string | undefined): string[] | undefined {
  if (value === undefined || value.trim() === '') return undefined;
  return value
    .split(',')
    .map(item => item.trim())
    .filter(Boolean);
}

function getPath(root: unknown, fieldPath: string): unknown {
  let current = root;
  for (const part of fieldPath.split('.')) {
    if (typeof current !== 'object' || current === null || Array.isArray(current)) {
      return undefined;
    }
    current = (current as Record<string, unknown>)[part];
  }
  return current;
}

function setPath(root: Record<string, unknown>, fieldPath: string, value: unknown): void {
  const parts = fieldPath.split('.');
  let current = root;
  for (const part of parts.slice(0, -1)) {
    const existing = current[part];
    if (typeof existing === 'object' && existing !== null && !Array.isArray(existing)) {
      current = existing as Record<string, unknown>;
    } else {
      /* v8 ignore start -- generated defaults contain every resolved section */
      const child: Record<string, unknown> = {};
      current[part] = child;
      current = child;
      /* v8 ignore stop */
    }
  }
  current[parts.at(-1)!] = value;
}

function clampNumber(field: ConfigFieldSpec, value: number): number {
  return Math.min(field.maximum!, Math.max(field.minimum!, value));
}

function isHttpUrl(value: string): boolean {
  try {
    return ['http:', 'https:'].includes(new URL(value).protocol);
  } catch {
    return false;
  }
}

function isLocalPath(value: string): boolean {
  const absolute =
    path.isAbsolute(value) ||
    /^~(?:[\\/]|$)/.test(value) ||
    /^[A-Za-z]:[\\/]/.test(value);
  return absolute && !value.split(/[\\/]/).includes('..');
}

function normalizeString(value: string, normalize?: 'trim' | 'lower'): string {
  const trimmed = value.trim();
  return normalize === 'lower' ? trimmed.toLowerCase() : trimmed;
}

function parseValue(
  field: ConfigFieldSpec,
  raw: unknown,
  fromEnvironment: boolean,
  normalize?: 'trim' | 'lower'
): { valid: boolean; value?: unknown } {
  switch (field.type) {
    case 'schemaVersion':
      return typeof raw === 'number' && Number.isInteger(raw)
        ? { valid: true, value: raw }
        : { valid: false };
    case 'boolean': {
      const value = fromEnvironment
        ? parseBooleanEnv(typeof raw === 'string' ? raw : undefined)
        : typeof raw === 'boolean'
          ? raw
          : undefined;
      return value === undefined ? { valid: false } : { valid: true, value };
    }
    case 'number': {
      const value = fromEnvironment
        ? parseIntEnv(typeof raw === 'string' ? raw : undefined)
        : typeof raw === 'number' && Number.isFinite(raw)
          ? Math.trunc(raw)
          : undefined;
      return value === undefined
        ? { valid: false }
        : { valid: true, value: clampNumber(field, value) };
    }
    case 'stringArray': {
      const value = fromEnvironment
        ? parseStringArrayEnv(typeof raw === 'string' ? raw : undefined)
        : raw === null || (Array.isArray(raw) && raw.every(item => typeof item === 'string'))
          ? raw
          : undefined;
      if (value === undefined) return { valid: false };
      if (field.itemFormat === 'path' && Array.isArray(value) && !value.every(isLocalPath)) {
        return { valid: false };
      }
      return { valid: true, value };
    }
    case 'enum': {
      if (typeof raw !== 'string') return { valid: false };
      const value = normalizeString(raw, normalize);
      return field.values?.includes(value)
        ? { valid: true, value }
        : { valid: false };
    }
    case 'url':
    case 'path':
    case 'string': {
      if (typeof raw !== 'string') return { valid: false };
      const value = fromEnvironment ? normalizeString(raw, normalize) : raw;
      if (value.trim() === '') return { valid: false };
      if (field.type === 'url' && !isHttpUrl(value)) return { valid: false };
      if (field.type === 'path' && !isLocalPath(value)) return { valid: false };
      return { valid: true, value };
    }
  }
}

/**
 * Resolve every contract field. `fileConfig` is one `.octocoderc` object or
 * the layers in priority order (workspace before global): per field the
 * environment wins, then the first layer holding a valid value, then defaults.
 */
export function resolveConfigFields(
  fileConfig: OctocodeConfig | readonly OctocodeConfig[] = {},
  env: Record<string, string | undefined> = process.env
): ResolvedConfigData {
  const layers: readonly OctocodeConfig[] = Array.isArray(fileConfig)
    ? fileConfig
    : [fileConfig as OctocodeConfig];
  const resolved = structuredClone(DEFAULT_CONFIG_VALUE) as unknown as Record<
    string,
    unknown
  >;

  for (const field of CONFIG_FIELDS) {
    if (!field.resolved) continue;
    let selected = false;

    for (const binding of field.env) {
      const raw = env[binding.name];
      if (raw === undefined) continue;
      const parsed = parseValue(field, raw, true, binding.normalize);
      if (parsed.valid) {
        setPath(resolved, field.path, parsed.value);
        selected = true;
        break;
      }
      if (binding.invalid === 'default') {
        setPath(resolved, field.path, field.defaultValue);
        selected = true;
        break;
      }
    }

    if (!selected && field.file) {
      for (const layer of layers) {
        const raw = getPath(layer, field.path);
        if (raw === undefined) continue;
        if (raw === null) {
          if (field.type !== 'stringArray') continue;
          setPath(resolved, field.path, null);
          selected = true;
          break;
        }
        const parsed = parseValue(field, raw, false);
        if (parsed.valid) {
          setPath(resolved, field.path, parsed.value);
          selected = true;
          break;
        }
      }
    }

  }

  return resolved as unknown as ResolvedConfigData;
}

export function resolveGitHub(
  fileConfig?: OctocodeConfig['github']
): RequiredGitHubConfig {
  return resolveConfigFields({ github: fileConfig }).github;
}

export function resolveLocal(
  fileConfig?: OctocodeConfig['local']
): RequiredLocalConfig {
  return resolveConfigFields({ local: fileConfig }).local;
}

export function resolveTools(
  fileConfig?: OctocodeConfig['tools']
): RequiredToolsConfig {
  return resolveConfigFields({ tools: fileConfig }).tools;
}

export function resolveNetwork(
  fileConfig?: OctocodeConfig['network']
): RequiredNetworkConfig {
  return resolveConfigFields({ network: fileConfig }).network;
}

export function resolveLsp(fileConfig?: OctocodeConfig['lsp']): RequiredLspConfig {
  return resolveConfigFields({ lsp: fileConfig }).lsp;
}

export function resolveOutput(
  fileConfig?: OctocodeConfig['output']
): RequiredOutputConfig {
  return resolveConfigFields({ output: fileConfig }).output;
}

export function resolveStorage(
  fileConfig?: OctocodeConfig['storage']
): RequiredStorageConfig {
  return resolveConfigFields({ storage: fileConfig }).storage;
}

