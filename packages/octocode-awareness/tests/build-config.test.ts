import { createRequire } from 'node:module';
import { describe, expect, it } from 'vitest';
// @ts-expect-error -- plain ESM build config, no type declarations needed
import { coreEntryPoints, external, skillScriptEntries } from '../buildConfig.mjs';

const require = createRequire(import.meta.url);
const pkg = require('../package.json') as { dependencies?: Record<string, string> };

describe('build config contract', () => {
  it('keeps zero npm runtime dependencies, so only Node builtins are external', () => {
    expect(Object.keys(pkg.dependencies ?? {})).toEqual([]);
    expect(external).toContain('fs');
    expect(external).toContain('node:fs');
    expect(external.every((specifier: string) => !specifier.startsWith('@octocodeai/'))).toBe(true);
  });

  it('generates exactly the four standalone skill bundles build.mjs marks as @generated', () => {
    const names = skillScriptEntries.map((entry: { outfileName: string }) => entry.outfileName);
    expect(names.sort()).toEqual([
      'awareness.mjs',
      'extract-hook-files.mjs',
      'hook-runner.mjs',
      'schema.mjs',
    ]);
  });

  it('every skill script entry has exactly one .ts entry point under bin/', () => {
    for (const entry of skillScriptEntries as Array<{ entryPoints: string[] }>) {
      expect(entry.entryPoints).toHaveLength(1);
      expect(entry.entryPoints[0]).toMatch(/^bin\/.+\.ts$/);
    }
  });

  it('core entry points cover the published CLI, hooks, and library surfaces', () => {
    expect(Object.keys(coreEntryPoints).sort()).toEqual([
      'extract-hook-files',
      'hook-runner',
      'index',
      'octocode-awareness',
      'schema',
      'schema-api',
    ]);
  });
});
