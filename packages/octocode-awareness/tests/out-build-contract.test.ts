import { describe, expect, it } from 'vitest';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const PACKAGE_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const read = (path: string) => readFileSync(resolve(PACKAGE_ROOT, path), 'utf8');

describe('Awareness out build contract', () => {
  it('owns the Awareness skill source inside the package', () => {
    expect(existsSync(resolve(PACKAGE_ROOT, 'skills/octocode-awareness/SKILL.md'))).toBe(true);
    expect(existsSync(resolve(PACKAGE_ROOT, '../../skills/octocode-awareness'))).toBe(false);
    expect(read('out/skills/octocode-awareness/SKILL.md')).toBe(read('skills/octocode-awareness/SKILL.md'));
  });

  it('publishes separate Awareness CLI and library entries from out', () => {
    const pkg = JSON.parse(read('package.json')) as {
      main?: string;
      types?: string;
      bin?: string | Record<string, string>;
      files?: string[];
      dependencies?: Record<string, string>;
    };

    expect(pkg.main).toBe('./out/index.js');
    expect(pkg.types).toBe('./out/types/src/index.d.ts');
    // Yarn 4 normalizes a single-entry bin object keyed by the unscoped package
    // name to string form on install; both forms name the binary octocode-awareness.
    const bin = typeof pkg.bin === 'string' ? { 'octocode-awareness': pkg.bin } : pkg.bin;
    expect(bin?.['octocode-awareness']).toBe('./out/octocode-awareness.js');
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
    const entries = read('build.entries.mjs'); // replaces buildConfig.mjs
    const source = `${read('src/index.ts')}\n${read('bin/awareness.ts')}`;

    // build.mjs delegates entry-point declarations to build.entries.mjs
    expect(build).toContain("from './build.entries.mjs'");
    expect(build).toMatch(/entryPoints:\s+coreEntryPoints/);
    expect(build).toMatch(/outdir:\s+outDir/);
    // New: builds directly to out/ — no temp staging directories
    expect(build).not.toContain('.out-build-');
    expect(build).not.toContain('renameSync');
    // Entry point declarations live in build.entries.mjs
    expect(entries).toContain("'octocode-awareness': 'bin/awareness.ts'");
    // Neither file bundles the external octocode CLI
    expect(build).not.toContain('packages/octocode/out');
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
    const bundledSkill = read('out/skills/octocode-awareness/SKILL.md');
    expect(bundledSkill).toContain('NOTICE → SCOPE/IDENTITY → INSPECT → ACT → OBSERVE → SETTLE/VERIFY → LEARN');
    expect(bundledSkill).toContain('## Start small');
    expect(bundledSkill).toContain('npx @octocodeai/octocode-awareness attend');
    expect(bundledSkill).toContain('After verification, reflect reusable lessons');
    expect(bundledSkill).not.toMatch(/Haiku|Composer 2\.5/);
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
