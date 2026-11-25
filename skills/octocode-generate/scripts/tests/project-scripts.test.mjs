import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { detectWorkspace } from '../detect-workspace.mjs';
import { applyTemplates, dependabotUpdates, listTemplates, outputPath, render } from '../apply-template.mjs';
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
  const dry = applyTemplates({ target: dir, templates: ['base', 'node-ts', 'lint-biome', 'ci-js'], pm: 'pnpm' });
  assert.equal(existsSync(join(dir, 'biome.json')), false);
  assert.equal(dry.files.find(file => file.path === '.gitignore').action, 'skip-exists');
  const applied = applyTemplates({ target: dir, templates: ['base', 'node-ts', 'lint-biome', 'ci-js'], pm: 'pnpm', write: true });
  assert.ok(readFileSync(join(dir, '.gitignore'), 'utf8').startsWith('mine\n'), 'existing content is kept; base only appends its lines');
  const pkg = JSON.parse(readFileSync(join(dir, 'package.json'), 'utf8'));
  assert.equal(pkg.scripts.test, 'vitest run', 'the npm init placeholder is replaced');
  assert.equal(pkg.scripts.lint, 'custom-lint', 'an existing script is kept');
  assert.deepEqual(applied.scripts.kept.map(entry => entry.name), ['lint']);
  assert.match(readFileSync(join(dir, '.github/workflows/ci.yml'), 'utf8'), /pnpm\/action-setup@[0-9a-f]{40}[\s\S]*run: pnpm run lint/);
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

test('templates: every workflow action is pinned to a full commit SHA with its release in a comment', () => {
  for (const pm of ['npm', 'pnpm', 'yarn', 'bun']) {
    const dir = fixture({});
    try {
      for (const template of listTemplates().map(entry => entry.name)) {
        applyTemplates({ target: dir, templates: [template], pm, write: true });
        for (const name of ['ci.yml']) {
          const path = join(dir, '.github', 'workflows', name);
          if (!existsSync(path)) continue;
          for (const line of readFileSync(path, 'utf8').split('\n').filter(text => /^\s*-?\s*uses:/.test(text))) {
            assert.match(line, /uses: [\w.-]+\/[\w./-]+@[0-9a-f]{40} # \S+$/, `${template} (${pm}): ${line.trim()}`);
          }
          rmSync(path);
        }
      }
    } finally { rmSync(dir, { recursive: true, force: true }); }
  }
});

test('templates: an equivalent existing config wins over the template file', () => withFixture({
  'package.json': { name: 'app' }, 'vite.config.ts': 'export default {}', 'prettier.config.mjs': 'export default {}',
}, dir => {
  const report = applyTemplates({ target: dir, templates: ['node-ts', 'lint-eslint'], write: true });
  const byPath = Object.fromEntries(report.files.map(file => [file.path, file]));
  assert.deepEqual([byPath['vitest.config.ts'].action, byPath['vitest.config.ts'].existing], ['skip-alternative', 'vite.config.ts']);
  assert.equal(byPath['.prettierrc.json'].existing, 'prettier.config.mjs');
  assert.equal(existsSync(join(dir, 'vitest.config.ts')), false);
  assert.equal(byPath['eslint.config.mjs'].action, 'create');
}));

test('templates: dependabot lists one ecosystem per detected stack, then GitHub Actions', () => withFixture({
  'package.json': {}, 'Cargo.toml': '[package]\nname = "x"\n', 'pyproject.toml': '[project]\nname = "x"\n', 'build.gradle.kts': '',
}, dir => {
  const ecosystems = [...dependabotUpdates(dir, 'pnpm').matchAll(/package-ecosystem: "([\w-]+)"/g)].map(match => match[1]);
  assert.deepEqual(ecosystems, ['npm', 'cargo', 'uv', 'gradle', 'github-actions']);
  assert.match(dependabotUpdates(dir, 'bun'), /"bun"/);
  const report = applyTemplates({ target: dir, templates: ['dependabot'], pm: 'pnpm', write: true });
  assert.equal(report.files[0].path, join('.github', 'dependabot.yml'));
  assert.match(readFileSync(join(dir, '.github', 'dependabot.yml'), 'utf8'), /^version: 2\nupdates:\n {2}- package-ecosystem: "npm"\n {4}directory: "\/"/);
}));

const listed = dir => Object.fromEntries(verifyProject(dir, { list: true }).gates.map(gate => [`${gate.stack}:${gate.gate}`, gate.command ?? gate.status]));

test('verify: Flutter, .NET, Gradle and Maven projects get gates from their manifests', () => {
  withFixture({ 'pubspec.yaml': 'name: app\ndependencies:\n  flutter:\n    sdk: flutter\n', 'test/widget_test.dart': '', 'web/index.html': '' }, dir => {
    const gates = listed(dir);
    assert.equal(gates['dart:lint+typecheck'], 'flutter analyze --fatal-infos');
    assert.equal(gates['dart:test'], 'flutter test');
    assert.equal(gates['dart:build'], 'flutter build web');
  });
  withFixture({ 'App.slnx': '', 'src/App/App.csproj': '<Project Sdk="Microsoft.NET.Sdk.Web" />' }, dir => {
    assert.equal(listed(dir)['dotnet:test'], 'missing', 'no test project: dotnet test would pass with nothing run');
  });
  withFixture({ 'App.slnx': '', 'tests/App.Tests/App.Tests.csproj': '<PackageReference Include="Microsoft.NET.Test.Sdk" />' }, dir => {
    assert.deepEqual([listed(dir)['dotnet:format+lint'], listed(dir)['dotnet:test']], ['dotnet format --verify-no-changes', 'dotnet test']);
  });
  withFixture({ 'settings.gradle.kts': '', gradlew: '', 'app/build.gradle.kts': 'plugins { alias(libs.plugins.android.application) }',
    'app/src/test/java/AppTest.kt': '' }, dir => {
    const gates = listed(dir);
    assert.deepEqual([gates['jvm:format'], gates['jvm:lint'], gates['jvm:test'], gates['jvm:build']],
      ['missing', './gradlew --console=plain lint', './gradlew --console=plain test', './gradlew --console=plain assemble']);
  });
  withFixture({ 'pom.xml': '<artifactId>spotless-maven-plugin</artifactId>' }, dir => {
    const gates = listed(dir);
    assert.deepEqual([gates['jvm:format'], gates['jvm:test'], gates['jvm:build']], ['mvn -B spotless:check', 'missing', 'mvn -B package -DskipTests']);
  });
});

test('verify: Swift, Ruby, PHP, Elixir and Deno projects get gates from their manifests', () => {
  withFixture({ 'Package.swift': '', 'Sources/A/a.swift': '', 'Tests/ATests/a.swift': '' }, dir => {
    assert.equal(listed(dir)['swift:format+lint'], 'swift format lint --strict --recursive Sources Tests Package.swift');
  });
  withFixture({ 'App.xcodeproj/project.pbxproj': '' }, dir => {
    const report = verifyProject(dir, { list: true });
    assert.ok(report.gates.every(gate => gate.status === 'missing' && /--gate/.test(gate.hint)));
  });
  withFixture({ Gemfile: 'gem "rails"\ngem "rubocop-rails-omakase", require: false\n', 'bin/rails': '', 'bin/rubocop': '' }, dir => {
    const gates = listed(dir);
    assert.deepEqual([gates['ruby:format+lint'], gates['ruby:test']], ['./bin/rubocop', './bin/rails test']);
  });
  withFixture({ 'composer.json': { require: { 'laravel/framework': '^13.0' }, 'require-dev': { 'laravel/pint': '^1', 'larastan/larastan': '^3' } }, artisan: '' }, dir => {
    const gates = listed(dir);
    assert.deepEqual([gates['php:format'], gates['php:lint+typecheck'], gates['php:test']], ['./vendor/bin/pint --test', './vendor/bin/phpstan analyse', 'php artisan test']);
  });
  withFixture({ 'mix.exs': 'defp deps, do: [{:credo, "~> 1.7"}]', 'test/app_test.exs': '' }, dir => {
    assert.deepEqual([listed(dir)['elixir:lint'], listed(dir)['elixir:build']], ['mix credo', 'mix compile --warnings-as-errors']);
  });
  withFixture({ 'deno.json': '{ "tasks": { "build": "vite build" } }', 'package.json': {} }, dir => {
    const report = verifyProject(dir, { list: true });
    assert.deepEqual(report.stacks, ['deno'], 'package.json in a Deno project is npm compatibility');
    assert.equal(listed(dir)['deno:build'], 'deno task build');
  });
});

test('verify: a missing tool is not-installed, and --gate replaces the derived gate through the shell', () => withFixture({
  'mix.exs': '', 'package.json': { scripts: { test: 'node -e ""' } },
}, dir => {
  const custom = verifyProject(dir, { only: ['test'], custom: [{ gate: 'test', command: 'node -e "process.exit(0)" && echo shell-ran' }] });
  assert.deepEqual(custom.gates.map(gate => `${gate.stack}:${gate.status}`), ['custom:pass'], 'derived test rows of every stack are replaced');
  const report = verifyProject(dir, { only: ['build'], custom: [{ gate: 'build', command: 'octocode-no-such-tool-xyz' }] });
  assert.equal(report.gates[0].status, 'fail', 'a shell gate reports the shell exit status');
}));

test('verify: a command that is not on PATH is not-installed with a hint', () => withFixture({
  'mix.exs': '', 'test/a_test.exs': '',
}, dir => {
  const hasMix = !spawnSync('mix', ['--version'], { stdio: 'ignore' }).error;
  const report = verifyProject(dir, { only: ['format'] });
  if (hasMix) assert.notEqual(report.gates[0].status, 'not-installed');
  else assert.deepEqual([report.gates[0].status, /not on PATH/.test(report.gates[0].hint)], ['not-installed', true]);
}));

test('detect: other ecosystems are languages, and their workspace manifests mark the root', () => withFixture({
  'settings.gradle.kts': 'include(":app")\n', 'Shop.slnx': '', 'pubspec.yaml': 'name: x\nworkspace:\n  - pkgs/a\n', 'mix.exs': '', Gemfile: '',
}, dir => {
  const report = detectWorkspace(join(dir, 'apps', 'new'), { probe: false });
  assert.equal(report.root, dir);
  assert.deepEqual(report.languages.toSorted(), ['dart', 'dotnet', 'elixir', 'jvm', 'ruby']);
  assert.deepEqual(report.monorepo.map(entry => entry.kind).toSorted(), ['dart workspace', 'dotnet solution', 'gradle']);
}));

test('detect: each toolchain probe uses its own version command', { skip: spawnSync('go', ['version'], { stdio: 'ignore' }).status !== 0 && 'go is not installed' }, () => {
  const report = detectWorkspace(tmpdir());
  assert.match(report.probes.go, /^go version go\d/);
});

test('templates: ci-js runs exactly the gate scripts the merged package.json has, for any framework', () => withFixture({
  'package.json': { name: 'ng-app', scripts: { build: 'ng build', test: 'ng test' } }, 'tsconfig.json': '{}',
}, dir => {
  applyTemplates({ target: dir, templates: ['node-ts', 'ci-js'], pm: 'pnpm', write: true });
  const ci = readFileSync(join(dir, '.github', 'workflows', 'ci.yml'), 'utf8');
  assert.match(ci, /- name: typecheck\n {8}run: pnpm run typecheck/, 'the typecheck script node-ts adds gets a step');
  assert.match(ci, /- name: test\n {8}run: pnpm run test/, 'the scaffold\'s own test script is kept and run');
  assert.match(ci, /# lint gate missing: add a lint script/);
  assert.doesNotMatch(ci, /run: pnpm run lint/, 'no step calls a script that does not exist');
}));

test('templates: ci-js on a framework app adds CI without a second test runner or tsconfig', () => withFixture({
  'package.json': { name: 'expo-app', scripts: { lint: 'expo lint', start: 'expo start' } }, 'tsconfig.json': '{}',
}, dir => {
  const report = applyTemplates({ target: dir, templates: ['base', 'ci-js', 'dependabot'], write: true });
  assert.deepEqual(report.scripts.added, [], 'no vitest or typecheck script is forced on the framework');
  const ci = readFileSync(join(dir, '.github', 'workflows', 'ci.yml'), 'utf8');
  assert.match(ci, /- name: lint\n {8}run: npm run lint/);
  assert.match(ci, /# test gate missing/);
}));

test('templates: base appends .octocode/ and .env to the scaffold\'s .gitignore without rewriting it', () => withFixture({
  '.gitignore': 'node_modules\n.env*\n',
}, dir => {
  const report = applyTemplates({ target: dir, templates: ['base', 'format-prettier'], write: true });
  assert.equal(readFileSync(join(dir, '.gitignore'), 'utf8'), 'node_modules\n.env*\n.octocode/\n.env\n!.env.example\n');
  assert.deepEqual(report.files.find(file => file.path === '.gitignore').lines, ['.octocode/', '.env', '!.env.example']);
  assert.equal(applyTemplates({ target: dir, templates: ['base'] }).files.find(file => file.path === '.gitignore').action, 'unchanged');
}));
