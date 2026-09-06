import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { getOctocodeHome } from '@octocodeai/octocode-shared/paths';

export const AWARENESS_CONFIG_VERSION = 1 as const;

export interface AwarenessFeatureConfig {
  hooks: boolean;
  notifications: boolean;
  verificationGate: boolean;
  sessionCapture: boolean;
  maintenanceReminders: boolean;
}

export interface AwarenessConfig {
  version: typeof AWARENESS_CONFIG_VERSION;
  features: AwarenessFeatureConfig;
}

export const DEFAULT_AWARENESS_CONFIG: AwarenessConfig = Object.freeze({
  version: AWARENESS_CONFIG_VERSION,
  features: Object.freeze({
    hooks: true,
    notifications: true,
    verificationGate: true,
    sessionCapture: true,
    maintenanceReminders: false,
  }),
});

export const AWARENESS_CONFIG_QUESTIONS = Object.freeze([
  { key: 'hooks', question: 'Enable Awareness host-hook automation?', default: true },
  { key: 'notifications', question: 'Allow hooks to deliver peer, handoff, and relevant memory context?', default: true },
  { key: 'verificationGate', question: 'Allow stop hooks to remind or block on unverified work?', default: true },
  { key: 'sessionCapture', question: 'Allow compact/end hooks to capture resumable session context?', default: true },
  { key: 'maintenanceReminders', question: 'Allow bounded maintenance-pressure reminders from hooks?', default: false },
] as const);

const FEATURE_KEYS = AWARENESS_CONFIG_QUESTIONS.map((entry) => entry.key);

export function awarenessConfigPath(env: NodeJS.ProcessEnv = process.env): string {
  return join(getOctocodeHome(env), 'awareness.json');
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function exactKeys(record: Record<string, unknown>, expected: readonly string[], label: string): void {
  const unknown = Object.keys(record).filter((key) => !expected.includes(key));
  const missing = expected.filter((key) => !Object.prototype.hasOwnProperty.call(record, key));
  if (unknown.length > 0 || missing.length > 0) {
    throw new Error(`${label} keys invalid; supported: ${expected.join(', ')}${unknown.length ? `; unknown: ${unknown.join(', ')}` : ''}${missing.length ? `; missing: ${missing.join(', ')}` : ''}`);
  }
}

export function parseAwarenessConfig(value: unknown): AwarenessConfig {
  if (!isRecord(value)) throw new Error('awareness config must be a JSON object');
  exactKeys(value, ['version', 'features'], 'top-level');
  if (value.version !== AWARENESS_CONFIG_VERSION) {
    throw new Error(`unsupported awareness config version: ${String(value.version)}; expected ${AWARENESS_CONFIG_VERSION}`);
  }
  const features = value.features;
  if (!isRecord(features)) throw new Error('features must be a JSON object');
  exactKeys(features, FEATURE_KEYS, 'features');
  for (const key of FEATURE_KEYS) {
    if (typeof features[key] !== 'boolean') throw new Error(`features.${key} must be boolean`);
  }
  return {
    version: AWARENESS_CONFIG_VERSION,
    features: Object.fromEntries(FEATURE_KEYS.map((key) => [key, features[key]])) as unknown as AwarenessFeatureConfig,
  };
}

export function loadAwarenessConfig(options: {
  env?: NodeJS.ProcessEnv;
  path?: string;
} = {}): { path: string; exists: boolean; config: AwarenessConfig } {
  const path = options.path ?? awarenessConfigPath(options.env);
  if (!existsSync(path)) return { path, exists: false, config: DEFAULT_AWARENESS_CONFIG };
  let parsed: unknown;
  try {
    parsed = JSON.parse(readFileSync(path, 'utf8'));
  } catch (error) {
    throw new Error(`cannot parse ${path}: ${(error as Error).message}`);
  }
  return { path, exists: true, config: parseAwarenessConfig(parsed) };
}

export function writeAwarenessConfig(config: AwarenessConfig, options: {
  env?: NodeJS.ProcessEnv;
  path?: string;
} = {}): string {
  const path = options.path ?? awarenessConfigPath(options.env);
  const validated = parseAwarenessConfig(config);
  mkdirSync(dirname(path), { recursive: true, mode: 0o700 });
  writeFileSync(path, `${JSON.stringify(validated, null, 2)}\n`, { encoding: 'utf8', flag: 'wx', mode: 0o600 });
  return path;
}

export function awarenessFeatureEnabled(
  feature: keyof AwarenessFeatureConfig,
  options: { env?: NodeJS.ProcessEnv; path?: string } = {},
): boolean {
  const loaded = loadAwarenessConfig(options);
  return loaded.exists && loaded.config.features[feature];
}
