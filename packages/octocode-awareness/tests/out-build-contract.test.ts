import { describe, expect, it } from 'vitest';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const PACKAGE_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const read = (path: string) => readFileSync(resolve(PACKAGE_ROOT, path), 'utf8');

describe('Awareness out build contract', () => {
  it('publishes separate Awareness CLI and library entries from out', () => {
    const pkg = JSON.parse(read('package.json')) as {
      main?: string;
      types?: string;
      bin?: Record<string, string>;
      files?: string[];
      dependencies?: Record<string, string>;
    };

    expect(pkg.main).toBe('./out/index.js');
    expect(pkg.types).toBe('./out/types/src/index.d.ts');
    expect(pkg.bin?.['octocode-awareness']).toBe('./out/octocode-awareness.js');
    expect(pkg.files).toContain('out/**');
    expect(pkg.files).not.toContain('dist/**');
    expect(pkg.dependencies ?? {}).not.toHaveProperty('octocode');
    expect(pkg.dependencies ?? {}).not.toHaveProperty('@octocodeai/octocode-tools-core');
  });

  it('keeps runtime logic and Zod schemas in TypeScript source', () => {
    const schemaDir = resolve(PACKAGE_ROOT, 'src/schema');
    expect(existsSync(schemaDir)).toBe(true);
    expect(readdirSync(schemaDir).filter((name) => name.endsWith('.ts')).length).toBeGreaterThan(5);
    expect(existsSync(resolve(PACKAGE_ROOT, 'scripts/schema.mjs'))).toBe(false);
    expect(read('src/schema/common.ts')).toContain("from 'zod'");
  });

  it('builds only Awareness-owned entries and never imports the octocode CLI', () => {
    const build = read('build.mjs');
    const buildConfig = read('buildConfig.mjs');
    const source = `${read('src/index.ts')}\n${read('bin/awareness.ts')}`;

    expect(build).toContain('entryPoints: coreEntryPoints');
    expect(buildConfig).toContain("'octocode-awareness': 'bin/awareness.ts'");
    expect(build).toContain('outdir: outDir');
    expect(build).toContain('.out-build-');
    expect(build).toContain('renameSync(outDir, publishedOutDir)');
    expect(build).not.toContain('rmSync(publishedOutDir');
    expect(build).not.toContain("packages/octocode/out");
    expect(source).not.toMatch(/from ['"]octocode(?:\/|['"])/);
    expect(source).not.toContain('@octocodeai/octocode-tools-core');
  });

  it('creates an executable CLI, import-only library, declarations, and bundled skill', () => {
    const cli = resolve(PACKAGE_ROOT, 'out/octocode-awareness.js');
    const library = resolve(PACKAGE_ROOT, 'out/index.js');
    expect(existsSync(cli)).toBe(true);
    expect(existsSync(library)).toBe(true);
    expect(existsSync(resolve(PACKAGE_ROOT, 'out/types/src/index.d.ts'))).toBe(true);
    expect(existsSync(resolve(PACKAGE_ROOT, 'out/skills/octocode-awareness/SKILL.md'))).toBe(true);
    expect(existsSync(resolve(PACKAGE_ROOT, 'dist'))).toBe(false);

    const schema = spawnSync(process.execPath, [cli, 'schema', 'list', '--compact'], {
      encoding: 'utf8',
      timeout: 10_000,
    });
    expect(schema.status, schema.stderr || schema.stdout).toBe(0);
    expect(JSON.parse(schema.stdout)).toContain('memory_recall');

    const imported = spawnSync(process.execPath, [
      '--input-type=module',
      '--eval',
      `const m = await import(${JSON.stringify(library)}); if (typeof m.getMemory !== 'function') process.exit(1);`,
    ], { encoding: 'utf8', timeout: 10_000 });
    expect(imported.status, imported.stderr || imported.stdout).toBe(0);
    expect(imported.stdout).toBe('');
  });
});
