// The package checks every host plugin (Claude Code, Codex) shares. The staged skills, manifests and
// licence are gitignored build output, so the checks build the plugin first and read only what that
// build wrote: a stale tree from an earlier skill change cannot fail them.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { existsSync, lstatSync, mkdtempSync, readFileSync, readdirSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { before, test } from 'node:test';
import { localScriptImports } from './bundled-imports.mjs';
import { readJson } from '../packages/octocode/scripts/build-plugin.mjs';

export interface PluginPackage {
  packageRoot: string;
  repoRoot: string;
  build: () => void;
  /** Manifest and MCP file, relative to the package root. */
  manifest: string;
  mcp: string;
  /** The host's marketplace catalog, relative to the repo root. */
  marketplace: string;
  /** Files the packed archive must carry besides the skills. */
  archiveRequired: string[];
}

const skillNames = (root: string): string[] =>
  readdirSync(root).filter(name => existsSync(join(root, name, 'SKILL.md'))).sort();

function filesUnder(root: string): string[] {
  return readdirSync(root).flatMap(name => {
    const path = join(root, name);
    assert.ok(!lstatSync(path).isSymbolicLink(), `Archive cannot depend on symlink: ${path}`);
    return lstatSync(path).isDirectory() ? filesUnder(path) : [path];
  });
}

export function definePluginPackageTests(plugin: PluginPackage): void {
  const { packageRoot, repoRoot } = plugin;
  before(() => plugin.build());

  test('release metadata, MCP pin, and marketplace agree', () => {
    const pkg = readJson(join(packageRoot, 'package.json'));
    const manifest = readJson(join(packageRoot, plugin.manifest));
    const server = readJson(join(packageRoot, plugin.mcp)).mcpServers.octocode;
    const catalog = readJson(join(repoRoot, plugin.marketplace));
    assert.equal(manifest.version, pkg.version);
    assert.equal(catalog.plugins[0].name, manifest.name);
    assert.deepEqual(catalog.plugins[0].source, { source: 'npm', package: pkg.name, version: pkg.version });
    assert.deepEqual(server, {
      type: 'stdio',
      command: 'npx',
      args: ['-y', `octocode-mcp@${readJson(join(repoRoot, 'packages/octocode-mcp/package.json')).version}`],
    });
  });

  test('all public skills ship with their local script imports', () => {
    const bundled = join(packageRoot, 'skills');
    assert.deepEqual(skillNames(bundled), skillNames(join(repoRoot, 'skills')));
    for (const name of skillNames(join(repoRoot, 'skills'))) {
      assert.equal(
        readFileSync(join(bundled, name, 'SKILL.md'), 'utf8'),
        readFileSync(join(repoRoot, 'skills', name, 'SKILL.md'), 'utf8')
      );
    }
    for (const path of filesUnder(bundled)) {
      assert.ok(!path.includes('/node_modules/') && !path.includes('/target/'));
      if (!/\.(?:mjs|cjs|js)$/.test(path)) continue;
      for (const specifier of localScriptImports(readFileSync(path, 'utf8'))) {
        const dependency = resolve(dirname(path), specifier);
        assert.ok(dependency.startsWith(`${bundled}/`), `Import escapes bundle: ${path}: ${specifier}`);
        assert.ok(existsSync(dependency), `Missing bundled import: ${path}: ${specifier}`);
      }
    }
  });

  test('npm archive is self-contained and excludes build sources and private files', () => {
    const temp = mkdtempSync(join(tmpdir(), 'octocode-plugin-pack-'));
    try {
      const [packed] = JSON.parse(
        execFileSync('npm', ['pack', '--ignore-scripts', '--json', '--pack-destination', temp], {
          cwd: packageRoot,
          encoding: 'utf8',
        })
      );
      const paths: string[] = packed.files.map((file: { path: string }) => file.path);
      for (const required of [...plugin.archiveRequired, 'LICENSE', 'skills/octocode-get-started/SKILL.md'])
        assert.ok(paths.includes(required), required);
      for (const path of paths)
        assert.ok(
          !/^(src|scripts|tests)\/|(^|\/)(\.env|credentials\.json|node_modules|\.git)(\/|$)/.test(path),
          path
        );
      assert.equal(
        paths.filter(path => /^skills\/[^/]+\/SKILL.md$/.test(path)).length,
        skillNames(join(packageRoot, 'skills')).length
      );
      execFileSync('tar', ['-xzf', join(temp, packed.filename), '-C', temp]);
      const installed = join(temp, 'package');
      assert.ok(existsSync(join(installed, 'skills/octocode-research/SKILL.md')));
      const review = join(installed, 'skills/octocode-skills/scripts/skill-review.mjs');
      const result = JSON.parse(
        execFileSync(process.execPath, [review, join(installed, 'skills'), '--json'], { encoding: 'utf8' })
      );
      assert.equal(result.errorCount, 0, 'Every installed skill must pass the structure and navigation review.');
    } finally {
      rmSync(temp, { recursive: true, force: true });
    }
  });
}
