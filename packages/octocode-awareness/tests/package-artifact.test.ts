import { afterEach, describe, expect, it } from 'vitest';
import { cpSync, existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const PACKAGE_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const tempRoots: string[] = [];

afterEach(() => {
  for (const root of tempRoots.splice(0)) {
    rmSync(root, { recursive: true, force: true });
  }
});

function isolatedPackage(): string {
  const root = mkdtempSync(join(tmpdir(), 'octocode-awareness-package-'));
  tempRoots.push(root);
  cpSync(resolve(PACKAGE_ROOT, 'out'), resolve(root, 'out'), { recursive: true });
  cpSync(resolve(PACKAGE_ROOT, 'README.md'), resolve(root, 'README.md'));
  if (existsSync(resolve(PACKAGE_ROOT, 'LICENSE'))) {
    cpSync(resolve(PACKAGE_ROOT, 'LICENSE'), resolve(root, 'LICENSE'));
  }
  writeFileSync(
    resolve(root, 'package.json'),
    readFileSync(resolve(PACKAGE_ROOT, 'package.json'), 'utf8'),
  );
  return root;
}

describe('published package artifact', () => {
  it('keeps one bundled skill tree and verifies the artifact during prepack', () => {
    const pkg = JSON.parse(readFileSync(resolve(PACKAGE_ROOT, 'package.json'), 'utf8')) as {
      files?: string[];
      scripts?: Record<string, string>;
      dependencies?: Record<string, string>;
    };

    expect(pkg.files).toContain('out/**');
    expect(pkg.files).not.toContain('skills/**');
    expect(pkg.scripts?.prepack).toContain('build');
    expect(pkg.scripts?.prepack).toContain('verify-package.mjs');
    expect(pkg.dependencies ?? {}).not.toHaveProperty('zod');
    expect(existsSync(resolve(PACKAGE_ROOT, 'LICENSE'))).toBe(true);
  });

  it('runs every schema command from an isolated package with no dependencies', { timeout: 30_000 }, () => {
    const root = isolatedPackage();
    const cli = resolve(root, 'out/octocode-awareness.js');
    const schema = resolve(root, 'out/skills/octocode-awareness/scripts/schema.mjs');

    const listed = spawnSync(process.execPath, [cli, 'schema', 'list', '--compact'], {
      cwd: root,
      encoding: 'utf8',
      timeout: 10_000,
    });
    expect(listed.status, listed.stderr || listed.stdout).toBe(0);
    const names = JSON.parse(listed.stdout) as string[];

    for (const name of names) {
      const jsonSchema = spawnSync(process.execPath, [schema, 'json-schema', name, '--compact'], {
        cwd: root,
        encoding: 'utf8',
        timeout: 5_000,
      });
      expect(jsonSchema.status, `${name}: ${jsonSchema.stderr || jsonSchema.stdout}`).toBe(0);

      const example = spawnSync(process.execPath, [schema, 'example', name, '--compact'], {
        cwd: root,
        encoding: 'utf8',
        timeout: 5_000,
      });
      expect(example.status, `${name}: ${example.stderr || example.stdout}`).toBe(0);

      const validation = spawnSync(process.execPath, [schema, 'validate', name, '-', '--compact'], {
        cwd: root,
        encoding: 'utf8',
        input: example.stdout,
        timeout: 5_000,
      });
      expect(validation.status, `${name}: ${validation.stderr || validation.stdout}`).toBe(0);
    }
  });
});
