#!/usr/bin/env node
// Derive a project's quality gates from its manifests and run them: format, lint, typecheck, test, build.
import { existsSync, readFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { join, resolve } from 'node:path';
import { fail, errorMessage } from './lib/fail.mjs';
import { isMainModule } from './lib/entrypoint.mjs';
import { detectWorkspace } from './detect-workspace.mjs';

const HELP = `Usage: node <skill-folder>/scripts/verify-project.mjs <project-dir> [--list] [--only <gates>] [--timeout <seconds>]

Finds each stack in the project (package.json, Cargo.toml, pyproject.toml, go.mod) and its gates:
  format, lint, typecheck, test, build
JavaScript gates are package.json scripts run with the detected package manager: format:check (or
fmt:check, check:format), lint, typecheck (or type-check), test, build; a "check" script stands in for
missing format, lint and typecheck gates. Rust, Python (uv) and Go gates are their standard commands.
A gate the project lacks is "missing": add it rather than skipping it.

Options:
  --list               Print the gates and commands; run nothing
  --only <gates>       Comma-separated gate names to run (e.g. lint,test)
  --timeout <seconds>  Per-gate limit (default 600)
  --help               Show this help

Output: one JSON object on stdout.
  ok      true when every gate passed or is not-applicable
  dir, packageManager
  gates   [{stack, gate, command, status, exitCode, durationMs, tail}]: status pass, fail, missing,
          not-applicable, timeout or listed; tail is the last 40 output lines of a failed gate
Effects: runs the project's own commands (third-party code) with CI=1 in <project-dir>; --list runs nothing.
Exit codes: 0 ok; 1 a gate failed, timed out or is missing; 2 invalid input, as {"ok":false,"error":{code,message}}.`;

const GATES = ['format', 'lint', 'typecheck', 'test', 'build'];
const SCRIPT_NAMES = { format: ['format:check', 'fmt:check', 'check:format'], lint: ['lint'], typecheck: ['typecheck', 'type-check'], test: ['test'], build: ['build'] };
const PLACEHOLDER_TEST = /no test specified/;
// Color sequences some tools print despite NO_COLOR; built from the ESC code to keep control characters out of the source.
const ANSI_COLOR = new RegExp(`${String.fromCharCode(27)}\\[[0-9;]*m`, 'g');

const readText = file => { try { return readFileSync(file, 'utf8'); } catch { return ''; } };
const onPath = tool => !spawnSync(tool, ['--version'], { stdio: 'ignore', timeout: 10_000 }).error;

function nodeGates(dir, pm) {
  const pkg = JSON.parse(readText(join(dir, 'package.json')) || '{}');
  const scripts = pkg.scripts ?? {};
  const run = name => [pm, 'run', name];
  const hasTs = existsSync(join(dir, 'tsconfig.json'));
  const gates = GATES.map(gate => {
    const script = SCRIPT_NAMES[gate].find(name => scripts[name] && !(gate === 'test' && PLACEHOLDER_TEST.test(scripts[name])));
    if (script) return { gate, command: run(script) };
    if (gate === 'typecheck' && !hasTs) return { gate, status: 'not-applicable' };
    return { gate, status: 'missing' };
  });
  if (scripts.check) {
    const covered = gates.filter(gate => gate.status === 'missing' && ['format', 'lint', 'typecheck'].includes(gate.gate));
    if (covered.length) {
      return [{ gate: covered.map(gate => gate.gate).join('+'), command: run('check') }, ...gates.filter(gate => !covered.includes(gate))];
    }
  }
  return gates;
}

function rustGates() {
  const nextest = spawnSync('cargo', ['nextest', '--version'], { stdio: 'ignore', timeout: 10_000 }).status === 0;
  return [
    { gate: 'format', command: ['cargo', 'fmt', '--all', '--check'] },
    { gate: 'lint', command: ['cargo', 'clippy', '--all-targets', '--all-features', '--', '-D', 'warnings'] },
    { gate: 'typecheck', status: 'not-applicable' },
    { gate: 'test', command: nextest ? ['cargo', 'nextest', 'run', '--all-features'] : ['cargo', 'test', '--all-features'] },
    { gate: 'build', command: ['cargo', 'build', '--all-targets'] },
  ];
}

function pythonGates(dir) {
  const pyproject = readText(join(dir, 'pyproject.toml'));
  const has = name => new RegExp(`["'\\[.]${name}(["'\\]<>=~! .]|$)`, 'm').test(pyproject);
  const checker = has('mypy') ? ['mypy', '.'] : has('pyright') ? ['pyright'] : has('ty') ? ['ty', 'check'] : null;
  return [
    has('ruff') ? { gate: 'format', command: ['uv', 'run', 'ruff', 'format', '--check'] } : { gate: 'format', status: 'missing' },
    has('ruff') ? { gate: 'lint', command: ['uv', 'run', 'ruff', 'check'] } : { gate: 'lint', status: 'missing' },
    checker ? { gate: 'typecheck', command: ['uv', 'run', ...checker] } : { gate: 'typecheck', status: 'missing' },
    has('pytest') ? { gate: 'test', command: ['uv', 'run', 'pytest'] } : { gate: 'test', status: 'missing' },
    /^\[build-system\]/m.test(pyproject) ? { gate: 'build', command: ['uv', 'build'] } : { gate: 'build', status: 'not-applicable' },
  ];
}

function goGates(dir) {
  const golangci = ['.golangci.yml', '.golangci.yaml', '.golangci.toml', '.golangci.json'].some(file => existsSync(join(dir, file)));
  return [
    { gate: 'format', command: ['gofmt', '-l', '.'], emptyOutput: true },
    { gate: 'lint', command: golangci || onPath('golangci-lint') ? ['golangci-lint', 'run'] : ['go', 'vet', './...'] },
    { gate: 'typecheck', status: 'not-applicable' },
    { gate: 'test', command: ['go', 'test', '-race', './...'] },
    { gate: 'build', command: ['go', 'build', './...'] },
  ];
}

/** Gates per stack found in dir, with the package manager used for JavaScript scripts. */
export function planGates(dir) {
  const workspace = detectWorkspace(dir, { probe: false });
  const pm = workspace.packageManager?.name ?? 'npm';
  const stacks = [];
  if (existsSync(join(dir, 'package.json'))) stacks.push(['javascript', nodeGates(dir, pm)]);
  if (existsSync(join(dir, 'Cargo.toml'))) stacks.push(['rust', rustGates()]);
  if (existsSync(join(dir, 'pyproject.toml'))) stacks.push(['python', pythonGates(dir)]);
  if (existsSync(join(dir, 'go.mod'))) stacks.push(['go', goGates(dir)]);
  return { packageManager: pm, gates: stacks.flatMap(([stack, gates]) => gates.map(gate => ({ stack, ...gate }))) };
}

function runGate(dir, gate, timeoutMs) {
  const started = Date.now();
  const [command, ...args] = gate.command;
  const result = spawnSync(command, args, { cwd: dir, encoding: 'utf8', timeout: timeoutMs, maxBuffer: 64 * 1024 * 1024,
    stdio: ['ignore', 'pipe', 'pipe'], env: { ...process.env, CI: '1', FORCE_COLOR: '0', NO_COLOR: '1', COREPACK_ENABLE_DOWNLOAD_PROMPT: '0' } });
  const output = `${result.stdout ?? ''}${result.stderr ?? ''}`.replace(ANSI_COLOR, '');
  const timedOut = result.error?.code === 'ETIMEDOUT' || result.signal === 'SIGTERM';
  const passed = !result.error && result.status === 0 && (!gate.emptyOutput || !result.stdout.trim());
  const status = timedOut ? 'timeout' : passed ? 'pass' : 'fail';
  return { status, exitCode: result.status, durationMs: Date.now() - started,
    ...(status === 'pass' ? {} : { tail: (result.error && !timedOut ? `${result.error.message}\n` : '') + output.trimEnd().split('\n').slice(-40).join('\n') }) };
}

/** Plan, and unless list, run the gates. */
export function verifyProject(target, { list = false, only = null, timeoutSeconds = 600 } = {}) {
  const dir = resolve(target);
  const { packageManager, gates } = planGates(dir);
  const selected = only ? gates.filter(gate => gate.gate.split('+').some(name => only.includes(name))) : gates;
  const results = selected.map(gate => {
    const row = { stack: gate.stack, gate: gate.gate, command: gate.command?.join(' ') ?? null };
    if (gate.status) return { ...row, status: gate.status };
    if (list) return { ...row, status: 'listed' };
    return { ...row, ...runGate(dir, gate, timeoutSeconds * 1000) };
  });
  const ok = results.length > 0 && results.every(row => ['pass', 'not-applicable', 'listed'].includes(row.status));
  return { ok, dir, packageManager, gates: results, ...(results.length ? {} : { error: { code: 'no-stack', message: 'no package.json, Cargo.toml, pyproject.toml or go.mod found' } }) };
}

function parseArgs(argv) {
  const options = { list: false, only: null, timeoutSeconds: 600 };
  const positional = [];
  for (let index = 0; index < argv.length; index++) {
    const arg = argv[index];
    if (arg === '--list') options.list = true;
    else if (arg === '--only' || arg === '--timeout') {
      const value = argv[++index];
      if (!value) throw new Error(`${arg} needs a value`);
      if (arg === '--only') {
        options.only = value.split(',').map(name => name.trim()).filter(Boolean);
        const unknown = options.only.filter(name => !GATES.includes(name));
        if (unknown.length) throw new Error(`unknown gate ${unknown.join(', ')}; gates: ${GATES.join(', ')}`);
      } else {
        options.timeoutSeconds = Number(value);
        if (!Number.isInteger(options.timeoutSeconds) || options.timeoutSeconds <= 0) throw new Error('--timeout needs a positive whole number of seconds');
      }
    } else if (arg.startsWith('--')) throw new Error(`unknown option ${arg}`);
    else positional.push(arg);
  }
  if (positional.length !== 1) throw new Error('give exactly one project directory');
  if (!existsSync(positional[0])) throw new Error(`no such directory: ${positional[0]}`);
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
