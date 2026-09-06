import { createRequire } from 'node:module';
import { describe, expect, it } from 'vitest';
// @ts-expect-error -- plain ESM build entries, no type declarations needed
import { coreEntryPoints, skillScriptEntries } from '../build.entries.mjs';

// @ts-expect-error -- plain ESM build options
import { nodeExternals } from '../build-options.mjs';

const require = createRequire(import.meta.url);
const pkg = require('../package.json') as { dependencies?: Record<string, string>; exports: Record<string, unknown> };

describe('build config contract', () => {
  it('keeps zero npm runtime dependencies, so only Node builtins are external', () => {
    expect(Object.keys(pkg.dependencies ?? {})).toEqual([]);
    expect(Object.keys(pkg.exports).sort()).toEqual(['.', './bin/awareness', './bin/extract-hook-files', './schema']);
    expect(nodeExternals).toContain('fs');
    expect(nodeExternals).toContain('node:fs');
    expect(nodeExternals.every((specifier: string) => !specifier.startsWith('@octocodeai/'))).toBe(true);
  });

  it('generates exactly the three standalone skill bundles build.mjs marks as @generated', () => {
    const names = skillScriptEntries.map((entry: { outfileName: string }) => entry.outfileName);
    expect(names.sort()).toEqual([
      'awareness.mjs',
      'extract-hook-files.mjs',
      'hook-runner.mjs',
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
      'schema-api',
    ]);
  });
});
