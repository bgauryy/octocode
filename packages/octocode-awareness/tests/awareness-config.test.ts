import { afterEach, describe, expect, it, vi } from 'vitest';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { fileURLToPath } from 'node:url';
import {
  AWARENESS_CONFIG_QUESTIONS,
  DEFAULT_AWARENESS_CONFIG,
  awarenessConfigPath,
  awarenessFeatureEnabled,
  loadAwarenessConfig,
  parseAwarenessConfig,
  writeAwarenessConfig,
} from '../src/awareness-config.js';
import { runHookCommand } from '../src/hooks/runner.js';
import { AWARENESS_INTEGRATION_HOSTS, DEFAULT_WORKSPACE_POLICY } from '../src/workspace-policy.js';

vi.mock('../src/hooks/history-capture.js', () => ({ captureHookHistory: vi.fn() }));

afterEach(() => vi.unstubAllEnvs());

describe('awareness configuration', () => {
  it('uses explicit defaults when awareness.json is missing', () => {
    const home = mkdtempSync(join(tmpdir(), 'awareness-config-'));
    try {
      const loaded = loadAwarenessConfig({ env: { OCTOCODE_HOME: home } });
      expect(loaded).toEqual({
        path: join(home, 'awareness.json'),
        exists: false,
        config: DEFAULT_AWARENESS_CONFIG,
      });
      expect(AWARENESS_CONFIG_QUESTIONS).toHaveLength(5);
      expect(awarenessFeatureEnabled('hooks', { env: { OCTOCODE_HOME: home } })).toBe(true);
      expect(loaded.config.features).toEqual({ hooks: true, notifications: true, verificationGate: false, sessionCapture: false, maintenanceReminders: false });
    } finally { rmSync(home, { recursive: true, force: true }); }
  });

  it('writes a private, complete config without overwriting', () => {
    const home = mkdtempSync(join(tmpdir(), 'awareness-config-'));
    try {
      const path = writeAwarenessConfig(DEFAULT_AWARENESS_CONFIG, { env: { OCTOCODE_HOME: home } });
      expect(path).toBe(awarenessConfigPath({ OCTOCODE_HOME: home }));
      expect(JSON.parse(readFileSync(path, 'utf8'))).toEqual(DEFAULT_AWARENESS_CONFIG);
      expect(() => writeAwarenessConfig(DEFAULT_AWARENESS_CONFIG, { env: { OCTOCODE_HOME: home } })).toThrow();
    } finally { rmSync(home, { recursive: true, force: true }); }
  });

  it('rejects unknown, missing, and non-boolean feature values', () => {
    expect(() => parseAwarenessConfig({ version: 1, features: { ...DEFAULT_AWARENESS_CONFIG.features, future: true } })).toThrow(/unknown: future/);
    expect(() => parseAwarenessConfig({ version: 1, features: { hooks: true } })).toThrow(/missing:/);
    expect(() => parseAwarenessConfig({ version: 1, features: { ...DEFAULT_AWARENESS_CONFIG.features, hooks: 'yes' } })).toThrow(/must be boolean/);
  });

  it('keeps the bundled workspace-policy JSON Schema aligned with canonical defaults', () => {
    const schemaPath = resolve(
      dirname(fileURLToPath(import.meta.url)),
      '../skills/octocode-awareness/references/awareness-config.schema.json',
    );
    const schema = JSON.parse(readFileSync(schemaPath, 'utf8')) as {
      additionalProperties: boolean;
      properties: {
        version: { const: number };
        storage: {
          additionalProperties: boolean;
          required: string[];
          properties: {
            repository: { enum: string[] };
            memory: { enum: string[] };
          };
        };
        hooks: {
          additionalProperties: boolean;
          required: string[];
          properties: {
            profile: { enum: string[] };
            owners: { additionalProperties: boolean; properties: Record<string, { $ref: string }> };
          };
        };
      };
    };
    expect(schema.additionalProperties).toBe(false);
    expect(schema.properties.version.const).toBe(1);
    expect(schema.properties.storage).toMatchObject({
      additionalProperties: false,
      required: Object.keys(DEFAULT_WORKSPACE_POLICY.storage),
    });
    expect(schema.properties.storage.properties.repository.enum).toEqual(['global', 'repo']);
    expect(schema.properties.storage.properties.memory.enum).toEqual(['global', 'repo']);
    expect(schema.properties.hooks).toMatchObject({ additionalProperties: false, required: ['profile'] });
    expect(schema.properties.hooks.properties.profile.enum).toContain(DEFAULT_WORKSPACE_POLICY.hooks.profile);
    expect(schema.properties.hooks.properties.owners.additionalProperties).toBe(false);
    expect(Object.keys(schema.properties.hooks.properties.owners.properties)).toEqual(AWARENESS_INTEGRATION_HOSTS);
    expect(new Set(Object.values(schema.properties.hooks.properties.owners.properties).map(value => value.$ref)))
      .toEqual(new Set(['#/$defs/owner']));
  });

  it('reports malformed files with their path', () => {
    const home = mkdtempSync(join(tmpdir(), 'awareness-config-'));
    const path = join(home, 'awareness.json');
    try {
      writeFileSync(path, '{');
      expect(() => loadAwarenessConfig({ path })).toThrow(path);
    } finally { rmSync(home, { recursive: true, force: true }); }
  });

  it('makes installed hook entrypoints inert when hooks are disabled', async () => {
    const home = mkdtempSync(join(tmpdir(), 'awareness-config-'));
    try {
      writeAwarenessConfig({
        ...DEFAULT_AWARENESS_CONFIG,
        features: { ...DEFAULT_AWARENESS_CONFIG.features, hooks: false },
      }, { env: { OCTOCODE_HOME: home } });
      vi.stubEnv('OCTOCODE_HOME', home);
      vi.stubEnv('OCTOCODE_AGENT_DIR', home);
      await expect(runHookCommand('pre-edit', '{ malformed payload is never parsed')).resolves.toBe(0);
    } finally { rmSync(home, { recursive: true, force: true }); }
  });
});
