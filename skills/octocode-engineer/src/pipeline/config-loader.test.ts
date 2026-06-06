import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

import { afterEach, describe, expect, it } from 'vitest';

import { loadConfigFile, mergeConfigIntoDefaults } from './config-loader.js';
import { DEFAULT_OPTS } from '../types/index.js';

describe('config-loader', () => {
  const tmpDirs: string[] = [];

  function makeTmpDir(): string {
    const d = fs.mkdtempSync(path.join(os.tmpdir(), 'config-test-'));
    tmpDirs.push(d);
    return d;
  }

  afterEach(() => {
    for (const d of tmpDirs) {
      fs.rmSync(d, { recursive: true, force: true });
    }
    tmpDirs.length = 0;
  });

  describe('loadConfigFile', () => {
    it('loads explicit config file', () => {
      const root = makeTmpDir();
      const cfgPath = path.join(root, 'my-config.json');
      fs.writeFileSync(cfgPath, JSON.stringify({ graph: true, semantic: true }));

      const config = loadConfigFile(root, cfgPath);
      expect(config).toBeDefined();
      expect(config!.graph).toBe(true);
      expect(config!.semantic).toBe(true);
    });

    it('auto-discovers .octocode-scan.json', () => {
      const root = makeTmpDir();
      fs.writeFileSync(
        path.join(root, '.octocode-scan.json'),
        JSON.stringify({ flow: true })
      );

      const config = loadConfigFile(root, null);
      expect(config).toBeDefined();
      expect(config!.flow).toBe(true);
    });

    it('auto-discovers .octocode-scan.jsonc', () => {
      const root = makeTmpDir();
      fs.writeFileSync(
        path.join(root, '.octocode-scan.jsonc'),
        '// comment\n{ "graph": true }\n'
      );

      const config = loadConfigFile(root, null);
      expect(config).toBeDefined();
      expect(config!.graph).toBe(true);
    });

    it('reads from package.json#octocode', () => {
      const root = makeTmpDir();
      fs.writeFileSync(
        path.join(root, 'package.json'),
        JSON.stringify({ name: 'test', octocode: { includeTests: true } })
      );

      const config = loadConfigFile(root, null);
      expect(config).toBeDefined();
      expect(config!.includeTests).toBe(true);
    });

    it('returns null when no config found', () => {
      const root = makeTmpDir();
      expect(loadConfigFile(root, null)).toBeNull();
    });

    it('converts kebab-case keys to camelCase', () => {
      const root = makeTmpDir();
      fs.writeFileSync(
        path.join(root, '.octocode-scan.json'),
        JSON.stringify({ 'include-tests': true, 'graph-advanced': true })
      );

      const config = loadConfigFile(root, null);
      expect(config!.includeTests).toBe(true);
      expect(config!.graphAdvanced).toBe(true);
    });

    it('prefers .octocode-scan.json over package.json', () => {
      const root = makeTmpDir();
      fs.writeFileSync(
        path.join(root, '.octocode-scan.json'),
        JSON.stringify({ graph: true })
      );
      fs.writeFileSync(
        path.join(root, 'package.json'),
        JSON.stringify({ name: 'x', octocode: { graph: false, flow: true } })
      );

      const config = loadConfigFile(root, null);
      expect(config!.graph).toBe(true);
      expect(config!.flow).toBeUndefined();
    });

    it('converts features string to Set', () => {
      const root = makeTmpDir();
      fs.writeFileSync(
        path.join(root, '.octocode-scan.json'),
        JSON.stringify({ features: 'architecture, dead-code' })
      );

      const config = loadConfigFile(root, null);
      expect(config!.features).toBeInstanceOf(Set);
      expect((config!.features as Set<string>).has('architecture')).toBe(true);
      expect((config!.features as Set<string>).has('dead-code')).toBe(true);
    });

    it('converts scope string to array', () => {
      const root = makeTmpDir();
      fs.writeFileSync(
        path.join(root, '.octocode-scan.json'),
        JSON.stringify({ scope: 'packages/foo,packages/bar' })
      );

      const config = loadConfigFile(root, null);
      expect(config!.scope).toEqual(['packages/foo', 'packages/bar']);
    });

    it('converts ignoreDirs array to Set', () => {
      const root = makeTmpDir();
      fs.writeFileSync(
        path.join(root, '.octocode-scan.json'),
        JSON.stringify({ 'ignore-dirs': ['vendor', 'generated'] })
      );

      const config = loadConfigFile(root, null);
      const dirs = config!.ignoreDirs as Set<string>;
      expect(dirs).toBeInstanceOf(Set);
      expect(dirs.has('vendor')).toBe(true);
      expect(dirs.has('generated')).toBe(true);
    });

    it('returns null for invalid JSON', () => {
      const root = makeTmpDir();
      fs.writeFileSync(
        path.join(root, '.octocode-scan.json'),
        'not { valid json !!!'
      );
      expect(loadConfigFile(root, null)).toBeNull();
    });

    it('resolves relative explicit path against root', () => {
      const root = makeTmpDir();
      const cfgPath = path.join(root, 'configs', 'scan.json');
      fs.mkdirSync(path.dirname(cfgPath), { recursive: true });
      fs.writeFileSync(cfgPath, JSON.stringify({ semantic: true }));

      const config = loadConfigFile(root, 'configs/scan.json');
      expect(config).toBeDefined();
      expect(config!.semantic).toBe(true);
    });

    it('strips single-line comments in JSONC', () => {
      const root = makeTmpDir();
      fs.writeFileSync(
        path.join(root, '.octocode-scan.json'),
        `{
  "graph": true,
  "flow": false
}`
      );
      const config = loadConfigFile(root, null);
      expect(config!.graph).toBe(true);
      expect(config!.flow).toBe(false);
    });

    it('strips block comments in JSONC', () => {
      const root = makeTmpDir();
      fs.writeFileSync(
        path.join(root, '.octocode-scan.json'),
        `{
  /* This enables the semantic phase */
  "semantic": true
}`
      );
      const config = loadConfigFile(root, null);
      expect(config!.semantic).toBe(true);
    });

    it('passes through threshold objects', () => {
      const root = makeTmpDir();
      fs.writeFileSync(
        path.join(root, '.octocode-scan.json'),
        JSON.stringify({ thresholds: { minFunctionStatements: 10 } })
      );

      const config = loadConfigFile(root, null);
      const t = config!.thresholds as Record<string, number>;
      expect(t.minFunctionStatements).toBe(10);
    });

    it('ignores package.json without octocode key', () => {
      const root = makeTmpDir();
      fs.writeFileSync(
        path.join(root, 'package.json'),
        JSON.stringify({ name: 'test', version: '1.0.0' })
      );
      expect(loadConfigFile(root, null)).toBeNull();
    });
  });

  describe('mergeConfigIntoDefaults', () => {
    it('config values override defaults', () => {
      const config = { graph: true, semantic: true };
      const cliArgs = { ...DEFAULT_OPTS };

      const result = mergeConfigIntoDefaults(DEFAULT_OPTS, config, cliArgs);
      expect(result.graph).toBe(true);
      expect(result.semantic).toBe(true);
    });

    it('CLI args override config when they differ from defaults', () => {
      const config = { graph: true, semantic: true };
      const cliArgs = { ...DEFAULT_OPTS, findingsLimit: 50 };

      const result = mergeConfigIntoDefaults(DEFAULT_OPTS, config, cliArgs);
      expect(result.graph).toBe(true);
      expect(result.semantic).toBe(true);
      expect(result.findingsLimit).toBe(50);
    });

    it('merges threshold overrides', () => {
      const config = { thresholds: { minFunctionStatements: 10 } };
      const cliArgs = { ...DEFAULT_OPTS };

      const result = mergeConfigIntoDefaults(DEFAULT_OPTS, config, cliArgs);
      expect(result.thresholds.minFunctionStatements).toBe(10);
      expect(result.thresholds.minFlowStatements).toBe(
        DEFAULT_OPTS.thresholds.minFlowStatements
      );
    });

    it('preserves defaults when config and CLI are empty', () => {
      const result = mergeConfigIntoDefaults(
        DEFAULT_OPTS,
        {},
        { ...DEFAULT_OPTS }
      );
      expect(result).toEqual(DEFAULT_OPTS);
    });

    it('config does not override root or packageRoot', () => {
      const config = { json: true };
      const cliArgs = { ...DEFAULT_OPTS };

      const result = mergeConfigIntoDefaults(DEFAULT_OPTS, config, cliArgs);
      expect(result.root).toBe(DEFAULT_OPTS.root);
      expect(result.json).toBe(true);
    });
  });
});
