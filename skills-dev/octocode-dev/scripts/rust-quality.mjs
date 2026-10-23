#!/usr/bin/env node
// Opt-in Rust quality tools for packages/octocode-native, pinned in its dev-tools.json.
// Usage: node skills-dev/octocode-dev/scripts/rust-quality.mjs <tools|pins|fuzz|miri|llvm-lines> [options]
//   tools [--json]                                   pinned vs installed; prints install commands, installs nothing
//   pins <tool...>                                   `name@version,...` for CI installs (taiki-e/install-action, cargo install)
//   fuzz [--smoke] [--seconds N] [--target-dir D] [target...]   libFuzzer on the pinned nightly, time-bounded
//   miri [--target-dir D]                            the selected tests under Miri, with a deadline
//   llvm-lines [package] [--timings] [--target-dir D] [-- extra]   LLVM IR lines per generic instantiation
import { spawnSync } from 'node:child_process';
import { mkdirSync, readdirSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { isMainModule } from './lib/entrypoint.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
export const NATIVE = path.resolve(HERE, '..', '..', '..', 'packages', 'octocode-native');

export function loadManifest(file = path.join(NATIVE, 'dev-tools.json')) {
  return JSON.parse(readFileSync(file, 'utf8'));
}

export function parseVersion(text) {
  return /\d+\.\d+\.\d+/.exec(text)?.[0] ?? null;
}

export function installCommand(tool) {
  return `cargo install --locked ${tool.name}@${tool.version}`;
}

/** `name@version` of each named tool, comma-joined; throws on a name the manifest does not pin. */
export function pinSpec(manifest, names) {
  if (names.length === 0) throw new Error('name at least one pinned tool');
  return names.map(name => {
    const tool = manifest.tools.find(entry => entry.name === name);
    if (!tool) throw new Error(`${name} is not pinned in dev-tools.json`);
    return `${tool.name}@${tool.version}`;
  }).join(',');
}

/** Each pinned tool against the installed one; `run` only executes version probes. */
export function inventory(manifest, run = probe) {
  return manifest.tools.map(tool => {
    const [command, ...args] = tool.versionCommand;
    const { status, stdout } = run(command, args);
    const have = status === 0 ? parseVersion(stdout) : null;
    const state = have === null ? 'missing' : have === tool.version ? 'ok' : 'mismatch';
    return { name: tool.name, want: tool.version, have, state, optional: Boolean(tool.optional), install: installCommand(tool), use: tool.use };
  });
}

function probe(command, args) {
  const result = spawnSync(command, args, { cwd: NATIVE, encoding: 'utf8' });
  return { status: result.status, stdout: `${result.stdout ?? ''}${result.stderr ?? ''}` };
}

const nightly = manifest => `+${manifest.toolchains.nightly.channel}`;

export function fuzzInvocation(manifest, { target, seconds, targetDir }) {
  const { dir, targets, libfuzzer } = manifest.fuzz;
  if (!targets.includes(target)) throw new Error(`unknown fuzz target "${target}" (known: ${targets.join(', ')})`);
  const corpus = `${dir}/corpus/${target}`;
  const seeds = `${dir}/seeds/${target}`;
  const args = [nightly(manifest), 'fuzz', 'run', '--fuzz-dir', dir, '--target-dir', targetDir ?? `${dir}/target`];
  return { command: 'cargo', args: [...args, target, corpus, seeds, '--', `-max_total_time=${seconds}`, ...libfuzzer], corpus };
}

export function miriInvocations(manifest, { targetDir }) {
  return manifest.miri.runs.map(run => {
    const tests = run.exact ? ['--exact', ...run.exact] : [run.filter];
    const skips = (run.skip ?? []).flatMap(name => ['--skip', name]);
    const env = { ...run.env, ...(run.miriflags && { MIRIFLAGS: run.miriflags }), ...(targetDir && { CARGO_TARGET_DIR: targetDir }) };
    const args = [nightly(manifest), 'miri', 'test', '--locked', '-p', run.package, '--lib', '--', ...tests, ...skips];
    return { command: 'cargo', args, env, why: run.why };
  });
}

export function llvmLinesInvocations(manifest, { pkg, timings, targetDir, extra = [] }) {
  const selection = ['--locked', '-p', pkg ?? manifest.llvmLines.package, ...manifest.llvmLines.args];
  const env = { CARGO_TARGET_DIR: path.resolve(NATIVE, targetDir ?? manifest.llvmLines.targetDir) };
  const lines = { command: 'cargo', args: ['llvm-lines', ...selection, ...extra], env };
  return timings ? [{ command: 'cargo', args: ['build', ...selection, '--timings'], env }, lines] : [lines];
}

/** Workspace package name → crate directory. */
export function crateDirs() {
  const crates = path.join(NATIVE, 'crates');
  return new Map(readdirSync(crates).flatMap(dir => {
    const name = /^name = "([^"]+)"/m.exec(readFileSync(path.join(crates, dir, 'Cargo.toml'), 'utf8'))?.[1];
    return name ? [[name, path.join(crates, dir)]] : [];
  }));
}

function parseOptions(argv) {
  const options = { positional: [], extra: [] };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === '--') {
      options.extra = argv.slice(index + 1);
      break;
    }
    if (arg === '--seconds' || arg === '--target-dir') options[arg.slice(2)] = argv[++index];
    else if (arg.startsWith('--')) options[arg.slice(2)] = true;
    else options.positional.push(arg);
  }
  return options;
}

function execute({ command, args, env }, timeoutSeconds) {
  console.error(`$ ${Object.entries(env ?? {}).map(([key, value]) => `${key}=${value} `).join('')}${command} ${args.join(' ')}`);
  const result = spawnSync(command, args, {
    cwd: NATIVE,
    stdio: 'inherit',
    env: { ...process.env, ...env },
    ...(timeoutSeconds && { timeout: timeoutSeconds * 1000, killSignal: 'SIGTERM' }),
  });
  if (result.error?.code === 'ETIMEDOUT') {
    console.error(`rust-quality: deadline of ${timeoutSeconds}s reached; stopped.`);
    return 124;
  }
  if (result.error) {
    console.error(`rust-quality: ${command} failed to start: ${result.error.message}`);
    return 1;
  }
  return result.status ?? 1;
}

/** Exit code 2 with the install commands when a needed tool or the nightly is absent. */
function requireTools(manifest, names, needsNightly) {
  const missing = inventory({ ...manifest, tools: manifest.tools.filter(tool => names.includes(tool.name)) })
    .filter(row => row.state !== 'ok')
    .map(row => `${row.name} ${row.have ?? 'missing'} (want ${row.want}): ${row.install}`);
  const { channel, install } = manifest.toolchains.nightly;
  if (needsNightly && spawnSync('rustup', ['run', channel, 'rustc', '--version'], { stdio: 'ignore' }).status !== 0) {
    missing.push(`${channel} toolchain missing: ${install}`);
  }
  if (missing.length === 0) return true;
  console.error(`rust-quality: install first (nothing installs automatically):\n  ${missing.join('\n  ')}`);
  process.exitCode = 2;
  return false;
}

function tools(manifest, options) {
  const rows = inventory(manifest);
  if (options.json) {
    console.log(JSON.stringify(rows, null, 2));
  } else {
    for (const row of rows) {
      const flag = row.state === 'ok' ? 'ok      ' : row.optional ? 'optional' : row.state.padEnd(8);
      const fix = row.state === 'ok' ? '' : `  → ${row.install}`;
      console.log(`${flag} ${row.name.padEnd(17)} want ${row.want.padEnd(8)} have ${(row.have ?? '-').padEnd(8)} ${row.use}${fix}`);
    }
    const { stable, nightly: pinned } = manifest.toolchains;
    console.log(`toolchains: stable ${stable.channel} (${stable.pinnedBy}); nightly ${pinned.channel} (${pinned.usedBy.join(', ')})`);
  }
  return rows.some(row => row.state !== 'ok' && !row.optional) ? 1 : 0;
}

function pins(manifest, options) {
  try {
    console.log(pinSpec(manifest, options.positional));
    return 0;
  } catch (error) {
    console.error(`rust-quality: ${error.message}`);
    return 1;
  }
}

function fuzz(manifest, options) {
  if (!requireTools(manifest, ['cargo-fuzz'], true)) return process.exitCode;
  const seconds = Number(options.seconds ?? (options.smoke ? manifest.fuzz.smokeSeconds : manifest.fuzz.seconds));
  const targets = options.positional.length > 0 ? options.positional : manifest.fuzz.targets;
  for (const target of targets) {
    const invocation = fuzzInvocation(manifest, { target, seconds, targetDir: options['target-dir'] });
    // libFuzzer grows the first corpus directory (gitignored) and only reads the seeds.
    mkdirSync(path.join(NATIVE, invocation.corpus), { recursive: true });
    const status = execute(invocation);
    if (status !== 0) return status;
  }
  return 0;
}

function miri(manifest, options) {
  if (!requireTools(manifest, [], true)) return process.exitCode;
  for (const invocation of miriInvocations(manifest, { targetDir: options['target-dir'] })) {
    console.error(`miri: ${invocation.why}`);
    const status = execute(invocation, manifest.miri.deadlineSeconds);
    if (status !== 0) return status;
  }
  return 0;
}

function llvmLines(manifest, options) {
  if (!requireTools(manifest, ['cargo-llvm-lines'], false)) return process.exitCode;
  const invocations = llvmLinesInvocations(manifest, {
    pkg: options.positional[0],
    timings: options.timings,
    targetDir: options['target-dir'],
    extra: options.extra,
  });
  for (const invocation of invocations) {
    const status = execute(invocation);
    if (status !== 0) return status;
  }
  return 0;
}

const COMMANDS = { tools, pins, fuzz, miri, 'llvm-lines': llvmLines };

function main() {
  const [name, ...rest] = process.argv.slice(2);
  const command = COMMANDS[name];
  if (!command) {
    console.error(`Usage: rust-quality.mjs <${Object.keys(COMMANDS).join('|')}> [options] (see the header of this file)`);
    process.exit(1);
  }
  process.exit(command(loadManifest(), parseOptions(rest)) ?? 0);
}

if (isMainModule(import.meta.url)) main();
