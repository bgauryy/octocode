import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { detectWorkspace } from '../detect-workspace.mjs';
import { applyTemplates, listTemplates, outputPath, render } from '../apply-template.mjs';
import { verifyProject } from '../verify-project.mjs';

function fixture(files) {
  const dir = mkdtempSync(join(tmpdir(), 'octocode-generate-'));
  mkdirSync(join(dir, '.git'));
  for (const [path, content] of Object.entries(files)) {
    mkdirSync(join(dir, path, '..'), { recursive: true });
    writeFileSync(join(dir, path), typeof content === 'string' ? content : JSON.stringify(content));
  }
  return dir;
}

const withFixture = (files, body) => { const dir = fixture(files); try { return body(dir); } finally { rmSync(dir, { recursive: true, force: true }); } };

test('detect: packageManager field wins over lockfiles, and every lockfile is listed', () => withFixture({
  'package.json': { packageManager: 'pnpm@12.23.0+sha512.abc', workspaces: ['packages/*'], devDependencies: { vitest: '^5' } },
  'pnpm-lock.yaml': '', 'package-lock.json': '{}', 'biome.json': '{}', '.eslintrc.json': '{}', 'tsconfig.json': '{}',
}, dir => {
  const report = detectWorkspace(dir, { probe: false });
  assert.deepEqual(report.packageManager, { name: 'pnpm', version: '12.23.0', source: 'packageManager' });
  assert.deepEqual(report.lockfiles.toSorted(), ['package-lock.json', 'pnpm-lock.yaml']);
  assert.ok(report.tools.biome && report.tools.eslintLegacy && report.tools.vitest);
  assert.deepEqual(report.languages.toSorted(), ['javascript', 'typescript']);
  assert.equal(report.monorepo[0].kind, 'package.json workspaces');
  assert.equal(report.probes, null);
}));

test('detect: a new folder inside a workspace reports the workspace root and its package manager', () => withFixture({
  'package.json': { workspaces: ['apps/*'] }, 'bun.lock': '', 'Cargo.toml': '[workspace]\nmembers = []\n',
}, dir => {
  const report = detectWorkspace(join(dir, 'apps', 'web'), { probe: false });
  assert.equal(report.target.exists, false);
  assert.equal(report.root, dir);
  assert.equal(report.packageManager.name, 'bun');
  assert.ok(report.monorepo.some(entry => entry.kind === 'cargo'));
}));

test('templates: paths map dot- and .tmpl, and placeholders indent multi-line values', () => {
  assert.equal(outputPath(join('dot-github', 'workflows', 'ci.yml.tmpl')), join('.github', 'workflows', 'ci.yml'));
  assert.equal(render('a:\n  {{block}}\nb: {{x}}', { block: 'one\ntwo', x: 1 }, 't'), 'a:\n  one\n  two\nb: 1');
  assert.throws(() => render('{{nope}}', {}, 't'), /unknown placeholder/);
  assert.equal(render('${{ github.ref }}', {}, 't'), '${{ github.ref }}', 'GitHub expressions are not placeholders');
});

test('templates: every shipped template renders for every package manager', () => {
  for (const pm of ['npm', 'pnpm', 'yarn', 'bun']) {
    const report = applyTemplates({ target: join(tmpdir(), 'octocode-generate-none'), templates: listTemplates().map(entry => entry.name), pm });
    assert.ok(report.files.length > 0);
  }
});

test('templates: a dry run writes nothing; --write creates, never overwrites, and merges scripts', () => withFixture({
  'package.json': { name: 'app', scripts: { test: 'echo "Error: no test specified" && exit 1', lint: 'custom-lint' } },
  '.gitignore': 'mine\n',
}, dir => {
  const dry = applyTemplates({ target: dir, templates: ['base', 'node-ts', 'lint-biome'], pm: 'pnpm' });
  assert.equal(existsSync(join(dir, 'biome.json')), false);
  assert.equal(dry.files.find(file => file.path === '.gitignore').action, 'skip-exists');
  const applied = applyTemplates({ target: dir, templates: ['base', 'node-ts', 'lint-biome'], pm: 'pnpm', write: true });
  assert.equal(readFileSync(join(dir, '.gitignore'), 'utf8'), 'mine\n');
  const pkg = JSON.parse(readFileSync(join(dir, 'package.json'), 'utf8'));
  assert.equal(pkg.scripts.test, 'vitest run', 'the npm init placeholder is replaced');
  assert.equal(pkg.scripts.lint, 'custom-lint', 'an existing script is kept');
  assert.deepEqual(applied.scripts.kept.map(entry => entry.name), ['lint']);
  assert.match(readFileSync(join(dir, '.github/workflows/ci.yml'), 'utf8'), /pnpm\/action-setup@v6[\s\S]*run: pnpm run lint/);
  assert.ok(applied.install.some(command => command.startsWith('pnpm add --save-dev @biomejs/biome')));
  const again = applyTemplates({ target: dir, templates: ['base'], pm: 'pnpm' });
  assert.ok(again.files.every(file => file.action === 'unchanged'));
}));

test('templates: unknown names and package managers are refused', () => {
  assert.throws(() => applyTemplates({ target: tmpdir(), templates: ['nope'] }), /unknown template/);
  assert.throws(() => applyTemplates({ target: tmpdir(), templates: ['base'], pm: 'pip' }), /--pm/);
});

test('verify: gates come from scripts, a check script covers missing ones, absent gates are missing', () => withFixture({
  'package.json': { scripts: { check: 'x', test: 'vitest run' } }, 'tsconfig.json': '{}', 'yarn.lock': '',
}, dir => {
  const report = verifyProject(dir, { list: true });
  assert.equal(report.packageManager, 'yarn');
  assert.deepEqual(report.gates.map(gate => `${gate.gate}:${gate.status}`), ['format+lint+typecheck:listed', 'test:listed', 'build:missing']);
  assert.equal(report.ok, false);
}));

test('verify: Rust, Python and Go projects get their standard gates', () => withFixture({
  'Cargo.toml': '[package]\nname = "x"\n', 'pyproject.toml': '[project]\nname = "x"\n[dependency-groups]\ndev = ["ruff>=0.17", "pytest"]\n', 'go.mod': 'module x\n',
}, dir => {
  const gates = verifyProject(dir, { list: true }).gates;
  assert.match(gates.find(gate => gate.stack === 'rust' && gate.gate === 'lint').command, /clippy .* -D warnings/);
  assert.deepEqual(gates.filter(gate => gate.stack === 'python').map(gate => gate.status), ['listed', 'listed', 'missing', 'listed', 'not-applicable']);
  assert.equal(gates.find(gate => gate.stack === 'go' && gate.gate === 'test').command, 'go test -race ./...');
}));

test('verify: runs gates and reports a failing one with its output tail', () => withFixture({
  'package.json': { scripts: { 'format:check': 'node -e ""', lint: 'node -e "console.log(\'bad rule\'); process.exit(3)"', test: 'node -e ""', build: 'node -e ""' } },
}, dir => {
  const report = verifyProject(dir, {});
  const byGate = Object.fromEntries(report.gates.map(gate => [gate.gate, gate]));
  assert.equal(byGate.format.status, 'pass');
  assert.equal(byGate.lint.status, 'fail');
  assert.match(byGate.lint.tail, /bad rule/);
  assert.equal(byGate.typecheck.status, 'not-applicable');
  assert.equal(report.ok, false);
  assert.equal(verifyProject(dir, { only: ['format', 'test'] }).ok, true);
}));
