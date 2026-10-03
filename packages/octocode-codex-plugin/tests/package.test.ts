import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import {
  existsSync,
  lstatSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { test } from 'node:test';
import { Ajv2020 } from 'ajv/dist/2020.js';
import { packageRoot, readJson, repoRoot } from '../scripts/build.ts';

const ajv = new Ajv2020({ allErrors: true, strict: false });
const names = (root: string) =>
  readdirSync(root)
    .filter(name => existsSync(join(root, name, 'SKILL.md')))
    .sort();
function files(root: string): string[] {
  return readdirSync(root).flatMap(name => {
    const path = join(root, name);
    assert.ok(
      !lstatSync(path).isSymbolicLink(),
      `Archive cannot depend on symlink: ${path}`
    );
    return lstatSync(path).isDirectory() ? files(path) : [path];
  });
}

test('manifests conform to upstream Agent Plugins schemas', () => {
  for (const name of ['plugin', 'mcp']) {
    const validate = ajv.compile(
      readJson(join(packageRoot, `tests/schemas/${name}.schema.json`))
    );
    assert.ok(
      validate(readJson(join(packageRoot, `${name}.json`))),
      ajv.errorsText(validate.errors)
    );
    assert.equal(
      validate({
        ...readJson(join(packageRoot, `${name}.json`)),
        unrecognizedField: true,
      }),
      false
    );
  }
});

test('release metadata, MCP pin, and marketplace agree', () => {
  const pkg = readJson(join(packageRoot, 'package.json'));
  const plugin = readJson(join(packageRoot, 'plugin.json'));
  const server = readJson(join(packageRoot, 'mcp.json')).mcpServers.octocode;
  const catalog = readJson(join(repoRoot, '.agents/plugins/marketplace.json'));
  assert.equal(plugin.version, pkg.version);
  assert.equal(catalog.plugins[0].name, plugin.name);
  assert.deepEqual(catalog.plugins[0].source, {
    source: 'npm',
    package: pkg.name,
    version: pkg.version,
  });
  assert.deepEqual(server, {
    type: 'stdio',
    command: 'npx',
    args: [
      '-y',
      `octocode-mcp@${readJson(join(repoRoot, 'packages/octocode-mcp/package.json')).version}`,
    ],
  });
  const extension = plugin.extensions['com.openai'];
  assert.ok(existsSync(resolve(packageRoot, extension.onboardingSkill)));
  for (const field of ['logo', 'composerIcon'])
    assert.ok(existsSync(resolve(packageRoot, extension.interface[field])));
  assert.ok(extension.interface.shortDescription.length <= 30);
  assert.ok(extension.interface.displayName.length <= 30);
});

test('all public skills ship with local script imports and generated helpers', () => {
  const bundled = join(packageRoot, 'skills');
  assert.deepEqual(
    names(bundled),
    [...names(join(repoRoot, 'skills')), 'octocode-get-started'].sort()
  );
  for (const name of names(join(repoRoot, 'skills'))) {
    assert.equal(
      readFileSync(join(bundled, name, 'SKILL.md'), 'utf8'),
      readFileSync(join(repoRoot, 'skills', name, 'SKILL.md'), 'utf8')
    );
  }
  for (const path of files(bundled)) {
    assert.ok(!path.includes('/node_modules/') && !path.includes('/target/'));
    if (!/\.(?:mjs|cjs|js)$/.test(path)) continue;
    const text = readFileSync(path, 'utf8');
    for (const match of text.matchAll(
      /(?:from\s*|import\s*\(|require\s*\()\s*['"](\.\.?\/[^'"\n]+)['"]/g
    )) {
      const dependency = resolve(dirname(path), match[1]);
      assert.ok(
        dependency.startsWith(`${bundled}/`),
        `Import escapes bundle: ${path}: ${match[1]}`
      );
      assert.ok(
        existsSync(dependency),
        `Missing bundled import: ${path}: ${match[1]}`
      );
    }
  }
});

test('npm archive is self-contained and excludes build sources and private files', () => {
  const temp = mkdtempSync(join(tmpdir(), 'octocode-plugin-pack-'));
  try {
    const packed = JSON.parse(
      execFileSync(
        'npm',
        ['pack', '--ignore-scripts', '--json', '--pack-destination', temp],
        {
          cwd: packageRoot,
          encoding: 'utf8',
          timeout: 30_000,
        }
      )
    )[0];
    const paths: string[] = packed.files.map(
      (file: { path: string }) => file.path
    );
    for (const required of [
      'plugin.json',
      'mcp.json',
      'assets/logo.png',
      'LICENSE',
      'skills/octocode-get-started/SKILL.md',
    ])
      assert.ok(paths.includes(required), required);
    for (const path of paths)
      assert.ok(
        !/^(src|scripts|tests)\/|(^|\/)(\.env|credentials\.json|node_modules|\.git)(\/|$)/.test(
          path
        ),
        path
      );
    assert.equal(
      paths.filter(path => /^skills\/[^/]+\/SKILL.md$/.test(path)).length,
      names(join(packageRoot, 'skills')).length
    );
    execFileSync('tar', ['-xzf', join(temp, packed.filename), '-C', temp]);
    const installed = join(temp, 'package');
    assert.ok(existsSync(join(installed, 'skills/octocode-research/SKILL.md')));
    const review = join(
      installed,
      'skills/octocode-skills/scripts/skill-review.mjs'
    );
    const result = JSON.parse(
      execFileSync(
        process.execPath,
        [review, join(installed, 'skills'), '--json'],
        { encoding: 'utf8', timeout: 30_000 }
      )
    );
    assert.equal(
      result.errorCount,
      0,
      'Every installed skill must pass the structure and navigation review.'
    );
  } finally {
    rmSync(temp, { recursive: true, force: true });
  }
});
