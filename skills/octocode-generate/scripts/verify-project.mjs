#!/usr/bin/env node
// Derive a project's quality gates from its manifests and run them: format, lint, typecheck, test, build.
import { existsSync, statSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { join, resolve } from 'node:path';
import { fail, errorMessage } from './lib/fail.mjs';
import { isMainModule } from './lib/entrypoint.mjs';
import { entries, findFiles, readJson, readText, stacksIn } from './lib/stacks.mjs';
import { detectWorkspace } from './detect-workspace.mjs';

const HELP = `Usage: node <skill-folder>/scripts/verify-project.mjs <project-dir> [--list] [--only <gates>] [--gate <gate>=<command>]... [--timeout <seconds>]

Finds each stack in the project folder and its gates: format, lint, typecheck, test, build.
  javascript  package.json scripts with the detected package manager: format:check (or fmt:check,
              check:format), lint, typecheck (or type-check), test, build; a "check" script stands
              in for missing format, lint and typecheck gates
  deno        deno fmt --check, deno lint, deno check, deno test -A, deno task build
  rust        cargo fmt, clippy -D warnings, nextest (or cargo test), cargo build
  python      uv run ruff format --check, ruff check, mypy/pyright/ty, pytest; uv build
  go          gofmt -l, golangci-lint (or go vet), go test -race, go build
  dart        dart format, flutter/dart analyze --fatal-infos, flutter/dart test, flutter build
  dotnet      dotnet format --verify-no-changes, dotnet test, dotnet build -warnaserror
  jvm         Gradle (./gradlew) or Maven (./mvnw) tasks for the plugins the build applies
  swift       swift format lint --strict, SwiftLint, swift test, swift build (Package.swift)
  ruby        RuboCop, Sorbet/Steep, rails test or RSpec
  php         Pint or PHP-CS-Fixer, PHPStan or Psalm, artisan test, Pest or PHPUnit
  elixir      mix format --check-formatted, mix credo, dialyzer, mix test, mix compile --warnings-as-errors
Combined rows ("format+lint") are one command that covers both gates. A gate the project lacks is
"missing" with a hint: add it rather than skipping it. A gate whose tool is not on PATH is "not-installed".

Options:
  --list                  Print the gates and commands; run nothing
  --only <gates>          Comma-separated gate names to run (e.g. lint,test)
  --gate <gate>=<command> Run <command> through the shell for <gate>, replacing the derived rows of
                          that gate (an Xcode scheme, a monorepo task); repeatable
  --timeout <seconds>     Per-gate limit (default 600)
  --help                  Show this help

Output: one JSON object on stdout.
  ok      true when every gate passed or is not-applicable
  dir, packageManager, stacks
  gates   [{stack, gate, command, status, exitCode, durationMs, tail, hint}]: status pass, fail, missing,
          not-installed, not-applicable, timeout or listed; tail is the last 40 output lines of a failed
          gate; hint says how to add a missing gate
Effects: runs the project's own commands (third-party code) with CI=1 in <project-dir>; --list runs nothing.
Exit codes: 0 ok; 1 a gate failed, timed out, is missing or not installed; 2 invalid input,
  as {"ok":false,"error":{code,message}}.`;

const GATES = ['format', 'lint', 'typecheck', 'test', 'build'];
const SCRIPT_NAMES = { format: ['format:check', 'fmt:check', 'check:format'], lint: ['lint'], typecheck: ['typecheck', 'type-check'], test: ['test'], build: ['build'] };
const PLACEHOLDER_TEST = /no test specified/;
// Color sequences some tools print despite NO_COLOR; built from the ESC code to keep control characters out of the source.
const ANSI_COLOR = new RegExp(`${String.fromCharCode(27)}\\[[0-9;]*m`, 'g');

const cmd = (gate, ...command) => ({ gate, command });
const missing = (gate, hint) => ({ gate, status: 'missing', hint });
const notApplicable = gate => ({ gate, status: 'not-applicable' });
const succeeds = (tool, args) => spawnSync(tool, args, { stdio: 'ignore', timeout: 10_000 }).status === 0;
const onPath = tool => !spawnSync(tool, ['--version'], { stdio: 'ignore', timeout: 10_000 }).error;

function nodeGates(dir, pm, override) {
  const scripts = override ?? readJson(join(dir, 'package.json'))?.scripts ?? {};
  const hasTs = existsSync(join(dir, 'tsconfig.json'));
  const hints = { format: 'add a format:check script (apply-template.mjs lint lane)', lint: 'add a lint script (apply-template.mjs lint lane)',
    typecheck: 'add "typecheck": "tsc -b --noEmit" (Next.js: "next typegen && tsc --noEmit")', test: 'add a test script and the first test', build: 'add a build script' };
  const gates = GATES.map(gate => {
    const script = SCRIPT_NAMES[gate].find(name => scripts[name] && !(gate === 'test' && PLACEHOLDER_TEST.test(scripts[name])));
    if (script) return cmd(gate, pm, 'run', script);
    if (gate === 'typecheck' && !hasTs) return notApplicable(gate);
    return missing(gate, hints[gate]);
  });
  const covered = scripts.check ? gates.filter(gate => gate.status === 'missing' && ['format', 'lint', 'typecheck'].includes(gate.gate)) : [];
  if (!covered.length) return gates;
  return [cmd(covered.map(gate => gate.gate).join('+'), pm, 'run', 'check'), ...gates.filter(gate => !covered.includes(gate))];
}

function denoGates(dir) {
  const config = readText(join(dir, 'deno.json')) || readText(join(dir, 'deno.jsonc'));
  const build = /"tasks"\s*:\s*\{[\s\S]*?"build"\s*:/.test(config);
  return [cmd('format', 'deno', 'fmt', '--check'), cmd('lint', 'deno', 'lint'), cmd('typecheck', 'deno', 'check'), cmd('test', 'deno', 'test', '-A'),
    build ? cmd('build', 'deno', 'task', 'build') : notApplicable('build')];
}

function rustGates() {
  const nextest = succeeds('cargo', ['nextest', '--version']);
  return [
    cmd('format', 'cargo', 'fmt', '--all', '--check'),
    cmd('lint', 'cargo', 'clippy', '--all-targets', '--all-features', '--', '-D', 'warnings'),
    notApplicable('typecheck'),
    nextest ? cmd('test', 'cargo', 'nextest', 'run', '--all-features') : cmd('test', 'cargo', 'test', '--all-features'),
    cmd('build', 'cargo', 'build', '--all-targets'),
  ];
}

function pythonGates(dir) {
  const pyproject = readText(join(dir, 'pyproject.toml'));
  const has = name => new RegExp(`["'\\[.]${name}(["'\\]<>=~! .]|$)`, 'm').test(pyproject);
  const checker = has('mypy') ? ['mypy', '.'] : has('pyright') ? ['pyright'] : has('ty') ? ['ty', 'check'] : null;
  const ruff = 'uv add --dev ruff (apply-template.mjs python)';
  return [
    has('ruff') ? cmd('format', 'uv', 'run', 'ruff', 'format', '--check') : missing('format', ruff),
    has('ruff') ? cmd('lint', 'uv', 'run', 'ruff', 'check') : missing('lint', ruff),
    checker ? cmd('typecheck', 'uv', 'run', ...checker) : missing('typecheck', 'uv add --dev mypy (or pyright)'),
    has('pytest') ? cmd('test', 'uv', 'run', 'pytest') : missing('test', 'uv add --dev pytest and write the first test'),
    /^\[build-system\]/m.test(pyproject) ? cmd('build', 'uv', 'build') : notApplicable('build'),
  ];
}

function goGates(dir) {
  const golangci = ['.golangci.yml', '.golangci.yaml', '.golangci.toml', '.golangci.json'].some(file => existsSync(join(dir, file)));
  return [
    { ...cmd('format', 'gofmt', '-l', '.'), emptyOutput: true },
    golangci || onPath('golangci-lint') ? cmd('lint', 'golangci-lint', 'run') : cmd('lint', 'go', 'vet', './...'),
    notApplicable('typecheck'),
    cmd('test', 'go', 'test', '-race', './...'),
    cmd('build', 'go', 'build', './...'),
  ];
}

// Flutter builds per platform folder: web needs no SDK, the host desktop needs only its toolchain, mobile last.
function flutterBuild(dir) {
  const host = { darwin: 'macos', linux: 'linux', win32: 'windows' }[process.platform];
  const targets = [{ folder: 'web', args: ['web'] }, { folder: host, args: [host] }, { folder: 'android', args: ['apk', '--debug'] },
    ...(host === 'macos' ? [{ folder: 'ios', args: ['ios', '--no-codesign'] }] : [])];
  const found = targets.find(({ folder }) => folder && existsSync(join(dir, folder)));
  return found ? cmd('build', 'flutter', 'build', ...found.args) : missing('build', 'add a platform: flutter create --platforms=web .');
}

function dartGates(dir) {
  const pubspec = readText(join(dir, 'pubspec.yaml'));
  const flutter = /^\s+sdk:\s*flutter\s*$/m.test(pubspec);
  const tool = flutter ? 'flutter' : 'dart';
  const name = /^name:\s*(\w+)/m.exec(pubspec)?.[1];
  const entry = name && existsSync(join(dir, 'bin', `${name}.dart`)) ? join('bin', `${name}.dart`) : null;
  return [
    cmd('format', 'dart', 'format', '--output=none', '--set-exit-if-changed', '.'),
    cmd('lint+typecheck', tool, 'analyze', '--fatal-infos'),
    existsSync(join(dir, 'test')) ? cmd('test', tool, 'test') : missing('test', 'write the first test under test/'),
    flutter ? flutterBuild(dir) : entry ? cmd('build', 'dart', 'compile', 'exe', entry, '-o', join('build', name)) : notApplicable('build'),
  ];
}

const DOTNET_TEST = /Include="(Microsoft\.NET\.Test\.Sdk|xunit[\w.]*|NUnit|MSTest[\w.]*|TUnit)"|Sdk="MSTest\.Sdk|<IsTestProject>\s*true/i;

function dotnetGates(dir) {
  const names = entries(dir);
  const solutions = names.filter(name => /\.slnx?$/.test(name)).toSorted();
  const projects = names.filter(name => /\.(cs|fs|vb)proj$/.test(name));
  // dotnet finds a lone solution or project itself; with several it needs one named.
  const target = solutions.length + projects.length > 1 ? solutions[0] ?? null : null;
  if (!solutions.length && projects.length > 1) {
    return GATES.map(gate => missing(gate, 'several projects and no solution: dotnet new sln, then dotnet sln add each project'));
  }
  const arg = target ? [target] : [];
  const tests = findFiles(dir, /\.(cs|fs|vb)proj$/, 4).some(file => DOTNET_TEST.test(readText(file)));
  return [
    cmd('format+lint', 'dotnet', 'format', ...arg, '--verify-no-changes'),
    notApplicable('typecheck'),
    tests ? cmd('test', 'dotnet', 'test', ...arg) : missing('test', 'add a test project (dotnet new xunit -o tests/<Name>.Tests) and add it to the solution'),
    cmd('build', 'dotnet', 'build', ...arg, '-warnaserror'),
  ];
}

function jvmGates(dir) {
  const names = entries(dir);
  const buildFiles = [...findFiles(dir, /^build\.gradle(\.kts)?$|^pom\.xml$/, 2), join(dir, 'gradle', 'libs.versions.toml')];
  const text = buildFiles.map(readText).join('\n');
  const has = pattern => pattern.test(text);
  const hasTests = findFiles(dir, /\.(java|kt)$/, 6).some(file => /[\\/]src[\\/](test|androidTest)[\\/]/.test(file));
  const testGate = runner => hasTests ? cmd('test', ...runner('test')) : missing('test', 'write the first test under src/test');
  if (names.some(name => /^(build|settings)\.gradle(\.kts)?$/.test(name))) {
    const gradle = task => [existsSync(join(dir, 'gradlew')) ? './gradlew' : 'gradle', '--console=plain', task];
    const format = has(/com\.diffplug\.spotless|spotless/) ? cmd('format', ...gradle('spotlessCheck'))
      : has(/org\.jlleitschuh\.gradle\.ktlint|ktlint/) ? cmd('format', ...gradle('ktlintCheck')) : missing('format', 'apply Spotless (com.diffplug.spotless) or ktlint');
    const lint = has(/com\.android\.(application|library)|android[.-](application|library)/) ? cmd('lint', ...gradle('lint'))
      : has(/detekt/) ? cmd('lint', ...gradle('detekt')) : has(/checkstyle/) ? cmd('lint', ...gradle('checkstyleMain'))
        : missing('lint', 'apply detekt (Kotlin) or Checkstyle (Java)');
    return [format, lint, notApplicable('typecheck'), testGate(gradle), cmd('build', ...gradle('assemble'))];
  }
  const maven = (...goals) => [existsSync(join(dir, 'mvnw')) ? './mvnw' : 'mvn', '-B', ...goals];
  return [
    has(/spotless-maven-plugin/) ? cmd('format', ...maven('spotless:check')) : missing('format', 'add spotless-maven-plugin'),
    has(/maven-checkstyle-plugin/) ? cmd('lint', ...maven('checkstyle:check')) : missing('lint', 'add maven-checkstyle-plugin'),
    notApplicable('typecheck'),
    testGate(goal => maven(goal)),
    cmd('build', ...maven('package', '-DskipTests')),
  ];
}

function swiftGates(dir) {
  const names = entries(dir);
  if (!names.includes('Package.swift')) {
    const hint = 'Xcode project: pass --gate <gate>="xcodebuild ... -scheme <Scheme>"; xcodebuild -list shows the schemes';
    return GATES.map(gate => missing(gate, hint));
  }
  const paths = ['Sources', 'Tests', 'Package.swift'].filter(path => existsSync(join(dir, path)));
  const swiftlint = names.some(name => /^\.swiftlint\.ya?ml$/.test(name));
  return [
    cmd(swiftlint ? 'format' : 'format+lint', 'swift', 'format', 'lint', '--strict', '--recursive', ...paths),
    ...(swiftlint ? [cmd('lint', 'swiftlint', 'lint', '--strict')] : []),
    notApplicable('typecheck'),
    existsSync(join(dir, 'Tests')) ? cmd('test', 'swift', 'test') : missing('test', 'add a test target under Tests/'),
    cmd('build', 'swift', 'build'),
  ];
}

function rubyGates(dir) {
  const gems = readText(join(dir, 'Gemfile'));
  const gem = name => new RegExp(`^\\s*gem\\s+["']${name}["']`, 'm').test(gems);
  const bin = (name, ...fallback) => existsSync(join(dir, 'bin', name)) ? [`./bin/${name}`] : ['bundle', 'exec', ...fallback];
  const rubocop = gem('rubocop') || gem('rubocop-rails-omakase') || gem('standard');
  return [
    rubocop ? cmd('format+lint', ...(gem('standard') && !gem('rubocop') ? ['bundle', 'exec', 'standardrb'] : bin('rubocop', 'rubocop'))) : missing('format+lint', 'add rubocop (or standard) to the Gemfile'),
    gem('sorbet') ? cmd('typecheck', 'bundle', 'exec', 'srb', 'tc') : gem('steep') ? cmd('typecheck', 'bundle', 'exec', 'steep', 'check') : notApplicable('typecheck'),
    existsSync(join(dir, 'bin', 'rails')) ? cmd('test', './bin/rails', 'test') : gem('rspec') || gem('rspec-rails') ? cmd('test', 'bundle', 'exec', 'rspec')
      : existsSync(join(dir, 'test')) ? cmd('test', 'bundle', 'exec', 'rake', 'test') : missing('test', 'add minitest or rspec and the first test'),
    notApplicable('build'),
  ];
}

function phpGates(dir) {
  const composer = readJson(join(dir, 'composer.json')) ?? {};
  const deps = { ...composer.require, ...composer['require-dev'] };
  const has = name => name in deps;
  const bin = name => `./vendor/bin/${name}`;
  return [
    has('laravel/pint') ? cmd('format', bin('pint'), '--test') : has('friendsofphp/php-cs-fixer') ? cmd('format', bin('php-cs-fixer'), 'fix', '--dry-run', '--diff')
      : missing('format', 'composer require --dev laravel/pint'),
    has('phpstan/phpstan') || has('larastan/larastan') ? cmd('lint+typecheck', bin('phpstan'), 'analyse') : has('vimeo/psalm') ? cmd('lint+typecheck', bin('psalm'))
      : missing('lint+typecheck', 'composer require --dev phpstan/phpstan (larastan/larastan for Laravel)'),
    existsSync(join(dir, 'artisan')) ? cmd('test', 'php', 'artisan', 'test') : has('pestphp/pest') ? cmd('test', bin('pest'))
      : has('phpunit/phpunit') ? cmd('test', bin('phpunit')) : missing('test', 'composer require --dev pestphp/pest and write the first test'),
    notApplicable('build'),
  ];
}

function elixirGates(dir) {
  const mix = readText(join(dir, 'mix.exs'));
  return [
    cmd('format', 'mix', 'format', '--check-formatted'),
    /:credo\b/.test(mix) ? cmd('lint', 'mix', 'credo') : missing('lint', 'add {:credo, only: [:dev, :test], runtime: false} to mix.exs'),
    /:dialyxir\b/.test(mix) ? cmd('typecheck', 'mix', 'dialyzer') : notApplicable('typecheck'),
    existsSync(join(dir, 'test')) ? cmd('test', 'mix', 'test') : missing('test', 'write the first test under test/'),
    cmd('build', 'mix', 'compile', '--warnings-as-errors'),
  ];
}

/** JavaScript gates for package.json scripts that are about to be written (apply-template.mjs renders CI from them). */
export function scriptGates(dir, pm, scripts) {
  return nodeGates(dir, pm, scripts);
}

/** Gates per stack found in dir, with the package manager used for JavaScript scripts. */
export function planGates(dir) {
  const pm = detectWorkspace(dir, { probe: false }).packageManager?.name ?? 'npm';
  const found = stacksIn(dir).filter(stack => !(stack === 'python' && !existsSync(join(dir, 'pyproject.toml'))));
  // A Deno project's package.json is npm compatibility, not a Node toolchain.
  const stacks = found.includes('deno') ? found.filter(stack => stack !== 'javascript') : found;
  const builders = { javascript: () => nodeGates(dir, pm), deno: () => denoGates(dir), rust: rustGates, python: () => pythonGates(dir), go: () => goGates(dir),
    dart: () => dartGates(dir), dotnet: () => dotnetGates(dir), jvm: () => jvmGates(dir), swift: () => swiftGates(dir), ruby: () => rubyGates(dir),
    php: () => phpGates(dir), elixir: () => elixirGates(dir) };
  return { packageManager: pm, stacks, gates: stacks.flatMap(stack => builders[stack]().map(gate => ({ stack, ...gate }))) };
}

function runGate(dir, gate, timeoutMs) {
  const started = Date.now();
  const [command, ...args] = gate.command;
  const result = spawnSync(command, args, { cwd: dir, encoding: 'utf8', timeout: timeoutMs, maxBuffer: 64 * 1024 * 1024, shell: gate.shell ?? false,
    stdio: ['ignore', 'pipe', 'pipe'], env: { ...process.env, CI: '1', FORCE_COLOR: '0', NO_COLOR: '1', COREPACK_ENABLE_DOWNLOAD_PROMPT: '0' } });
  const durationMs = Date.now() - started;
  if (result.error?.code === 'ENOENT') return { status: 'not-installed', exitCode: null, durationMs, hint: `${command} is not on PATH: install it, then rerun` };
  const output = `${result.stdout ?? ''}${result.stderr ?? ''}`.replace(ANSI_COLOR, '');
  const timedOut = result.error?.code === 'ETIMEDOUT' || result.signal === 'SIGTERM';
  const passed = !result.error && result.status === 0 && (!gate.emptyOutput || !result.stdout.trim());
  const status = timedOut ? 'timeout' : passed ? 'pass' : 'fail';
  return { status, exitCode: result.status, durationMs,
    ...(status === 'pass' ? {} : { tail: (result.error && !timedOut ? `${result.error.message}\n` : '') + output.trimEnd().split('\n').slice(-40).join('\n') }) };
}

/** Plan, and unless list, run the gates. custom is [{gate, command}] run through the shell, replacing derived rows of that gate. */
export function verifyProject(target, { list = false, only = null, custom = [], timeoutSeconds = 600 } = {}) {
  const dir = resolve(target);
  const { packageManager, stacks, gates } = planGates(dir);
  const replaced = new Set(custom.map(entry => entry.gate));
  const planned = [...gates.filter(gate => !replaced.has(gate.gate)), ...custom.map(entry => ({ stack: 'custom', gate: entry.gate, command: [entry.command], shell: true }))];
  const selected = only ? planned.filter(gate => gate.gate.split('+').some(name => only.includes(name))) : planned;
  const results = selected.map(gate => {
    const row = { stack: gate.stack, gate: gate.gate, command: gate.command?.join(' ') ?? null };
    if (gate.status) return { ...row, status: gate.status, ...(gate.hint ? { hint: gate.hint } : {}) };
    if (list) return { ...row, status: 'listed' };
    return { ...row, ...runGate(dir, gate, timeoutSeconds * 1000) };
  });
  const ok = results.length > 0 && results.every(row => ['pass', 'not-applicable', 'listed'].includes(row.status));
  return { ok, dir, packageManager, stacks, gates: results,
    ...(results.length ? {} : { error: { code: 'no-stack', message: 'no known project manifest found; pass --gate <gate>=<command> for each gate' } }) };
}

function parseArgs(argv) {
  const options = { list: false, only: null, custom: [], timeoutSeconds: 600 };
  const positional = [];
  for (let index = 0; index < argv.length; index++) {
    const arg = argv[index];
    if (arg === '--list') options.list = true;
    else if (['--only', '--timeout', '--gate'].includes(arg)) {
      const value = argv[++index];
      if (!value) throw new Error(`${arg} needs a value`);
      if (arg === '--only') {
        options.only = value.split(',').map(name => name.trim()).filter(Boolean);
        const unknown = options.only.filter(name => !GATES.includes(name));
        if (unknown.length) throw new Error(`unknown gate ${unknown.join(', ')}; gates: ${GATES.join(', ')}`);
      } else if (arg === '--gate') {
        const [, gate, command] = /^([a-z]+)=(.+)$/s.exec(value) ?? [];
        if (!GATES.includes(gate) || !command?.trim()) throw new Error(`--gate needs <gate>=<command> with a gate from ${GATES.join(', ')}`);
        options.custom.push({ gate, command });
      } else {
        options.timeoutSeconds = Number(value);
        if (!Number.isInteger(options.timeoutSeconds) || options.timeoutSeconds <= 0) throw new Error('--timeout needs a positive whole number of seconds');
      }
    } else if (arg.startsWith('--')) throw new Error(`unknown option ${arg}`);
    else positional.push(arg);
  }
  if (positional.length !== 1) throw new Error('give exactly one project directory');
  if (!existsSync(positional[0]) || !statSync(positional[0]).isDirectory()) throw new Error(`no such directory: ${positional[0]}`);
  return { target: positional[0], ...options };
}

if (isMainModule(import.meta.url)) {
  const argv = process.argv.slice(2);
  if (argv.includes('--help') || argv.includes('-h')) { console.log(HELP); process.exit(0); }
  let options;
  try { options = parseArgs(argv); } catch (error) { fail('usage', errorMessage(error), 2); }
  const report = verifyProject(options.target, options);
  console.log(JSON.stringify(report, null, 2));
  process.exit(report.ok ? 0 : 1);
}
