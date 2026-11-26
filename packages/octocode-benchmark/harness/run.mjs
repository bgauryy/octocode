#!/usr/bin/env node
// Runs a round: one fresh host session per pass, question and arm, as the round manifest describes.
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import {
  HarnessError, checkPins, cli, fill, isMain, loadManifest, parseArgs, passPath, pool, questionText, questions, realGh, removeTree,
  removeTreeAndEmptyParents, runChild, runName, runPath, sessionEnv, wrapperBin,
} from './lib.mjs';

const HELP = `Usage: node packages/octocode-benchmark/harness/run.mjs <round manifest> [options]

Starts one fresh session per pass, question and arm. Passes are interleaved: every arm answers pass 1 of every
question before any arm starts pass 2, and inside a pass the arms of one question run next to each other, in an
order that rotates per question. A run whose result.json exists is skipped, never overwritten.

  --pass N         only this pass
  --only A,B       only these questions (<set>-<id>, e.g. gh-Q1)
  --arms A,B       only these arms
  --jobs N         sessions at a time (default: the manifest's "jobs")
  --dry-run        print the ordered plan and the preflight result; start no session

Output: <campaign>/<pass dir>/runs/<set>-<id>-<arm>/{prompt.md, transcript.jsonl, answer.md, result.json}, a line
per run in <campaign>/<pass dir>/run.log, and gh-log.jsonl for shell arms. Per-run scratch (session home, MCP
config) lives under .octocode/tmp/<round>/ and is removed when the run ends.
Exit codes: 0 every run produced a result; 1 a run failed; 2 bad input or a failed preflight.`;

/** Every run of the round in start order: pass, then question, then arm (rotated per question and pass). */
export function plan(manifest, { pass, only, arms } = {}) {
  const tasks = [];
  const passes = pass === undefined ? Array.from({ length: manifest.passes }, (_, i) => i + 1) : [pass];
  for (const n of passes) {
    questions(manifest).forEach((question, at) => {
      if (only && !only.has(question.name)) return;
      const turn = (at + n - 1) % Math.max(1, question.arms.length);
      for (const arm of [...question.arms.slice(turn), ...question.arms.slice(0, turn)]) {
        if (arms && !arms.has(arm)) continue;
        tasks.push({ pass: n, set: question.set, id: question.id, arm, file: question.file, name: runName(question.set, question.id, arm) });
      }
    });
  }
  return tasks;
}

/** The host command line and environment for one run. */
export function session(manifest, task, { scratch, token, ghReal, ghBin }) {
  const arm = manifest.arms[task.arm];
  const prompt = `${questionText(task.file, { clones: manifest.clones })}\n\n${arm.instruction}`;
  const env = { ...sessionEnv(), GITHUB_TOKEN: token, RUN_ID: `p${task.pass}-${task.name}` };
  const args = ['-p', prompt, '--model', manifest.models.runner, '--setting-sources', '', '--no-session-persistence', '--output-format', 'stream-json', '--verbose'];
  let config;
  let home;
  if (arm.kind === 'shell') {
    env.PATH = `${ghBin}${path.delimiter}${env.PATH}`;
    env.BENCH_GH_REAL = ghReal;
    env.BENCH_GH_LOG = path.join(passPath(manifest, task.pass), 'gh-log.jsonl');
    args.push('--strict-mcp-config', '--tools', 'Bash', '--allowedTools', 'Bash(gh:*)', ...arm.filters.map(filter => `Bash(${filter}:*)`));
  } else {
    home = path.join(scratch, 'home');
    const pin = arm.pin === undefined ? undefined : manifest.pins[arm.pin].path;
    config = { mcpServers: { octocode: fill({ command: arm.server.command, args: arm.server.args ?? [], env: arm.server.env ?? {} }, manifest, { home, pin }) } };
    // toolSearch: the host keeps its tool search, so MCP schemas load on demand instead of all upfront.
    args.push('--mcp-config', path.join(scratch, 'mcp.json'), '--strict-mcp-config', '--tools', arm.toolSearch ? 'ToolSearch' : '', '--allowedTools', 'mcp__octocode', ...(arm.toolSearch ? ['ToolSearch'] : []));
  }
  return { prompt, env, args, config, home };
}

async function runOne(manifest, task, context) {
  const out = runPath(manifest, task.pass, task.set, task.id, task.arm);
  if (fs.existsSync(path.join(out, 'result.json'))) return { task, status: 'skip', ok: true };
  const scratch = path.join(manifest.scratch, `pass-${task.pass}`, task.name);
  removeTree(scratch);
  removeTree(out);
  fs.mkdirSync(out, { recursive: true });
  fs.mkdirSync(scratch, { recursive: true });
  const { prompt, env, args, config, home } = session(manifest, task, { ...context, scratch });
  if (home) fs.mkdirSync(home, { recursive: true });
  if (config) fs.writeFileSync(path.join(scratch, 'mcp.json'), JSON.stringify(config));
  fs.writeFileSync(path.join(out, 'prompt.md'), prompt);
  const started = Date.now();
  let ran;
  try {
    ran = await runChild(context.host, args, { cwd: manifest.neutralCwd, env, timeoutMs: manifest.timeouts.runSeconds * 1000 });
  } finally {
    removeTreeAndEmptyParents(scratch, manifest.scratch);
  }
  const wall = Math.round((Date.now() - started) / 100) / 10;
  fs.writeFileSync(path.join(out, 'transcript.jsonl'), ran.stdout);
  if (ran.stderr) fs.writeFileSync(path.join(out, 'stderr.txt'), ran.timedOut ? 'timeout' : ran.stderr);
  let result;
  for (const line of ran.stdout.split('\n')) {
    try {
      const event = JSON.parse(line);
      if (event?.type === 'result') result = event;
    } catch {
      // not an event line
    }
  }
  const { result: answer = '', ...rest } = result ?? {};
  fs.writeFileSync(path.join(out, 'answer.md'), answer ?? '');
  const arm = manifest.arms[task.arm];
  const record = { rc: ran.code, wall_s: wall, started: started / 1000, pass: task.pass, arm: task.arm, pin: arm.pin === undefined ? undefined : manifest.pins[arm.pin], home, result: rest };
  fs.writeFileSync(path.join(out, 'result.json'), JSON.stringify(record));
  return { task, ok: ran.code === 0 && Boolean(result), status: `rc=${ran.code} wall=${wall}s turns=${result?.num_turns} cost=${result?.total_cost_usd}` };
}

/** Checks everything a round depends on before the first session; throws on the first failure. */
export function preflight(manifest, tasks, host) {
  const arms = [...new Set(tasks.map(task => task.arm))].map(name => manifest.arms[name]);
  if (!manifest.models.runner) throw new HarnessError('round manifest: "models.runner" is missing');
  checkPins(manifest, [...new Set(arms.map(arm => arm.pin).filter(Boolean))]);
  for (const task of tasks) if (!fs.existsSync(task.file)) throw new HarnessError(`question file ${task.file} does not exist`);
  fs.mkdirSync(manifest.neutralCwd, { recursive: true });
  for (let at = manifest.neutralCwd; ; at = path.dirname(at)) {
    for (const name of ['CLAUDE.md', 'AGENTS.md']) {
      if (fs.existsSync(path.join(at, name))) throw new HarnessError(`neutralCwd ${manifest.neutralCwd} is under ${path.join(at, name)}: sessions would start with project instructions`);
    }
    if (at === path.dirname(at)) break;
  }
  if (fs.readdirSync(manifest.neutralCwd).length) throw new HarnessError(`neutralCwd ${manifest.neutralCwd} is not empty`);
  const version = spawnSync(host, ['--version'], { encoding: 'utf8' });
  if (version.error || version.status !== 0) throw new HarnessError(`host "${host} --version" failed: ${version.error?.message ?? version.stderr}`);
  const ghReal = realGh();
  const token = spawnSync(ghReal, ['auth', 'token'], { encoding: 'utf8' });
  if (token.status !== 0 || !token.stdout.trim()) throw new HarnessError(`"${ghReal} auth token" returned no token: run gh auth login`);
  const versions = { host: version.stdout.trim(), gh: spawnSync(ghReal, ['--version'], { encoding: 'utf8' }).stdout.split('\n')[0] };
  for (const arm of arms) {
    if (arm.kind !== 'mcp') continue;
    const pin = arm.pin === undefined ? undefined : manifest.pins[arm.pin].path;
    const server = fill(arm.server, manifest, { home: path.join(manifest.scratch, 'preflight-home'), pin });
    for (const arg of server.args ?? []) {
      if (path.isAbsolute(arg) && !fs.existsSync(arg)) throw new HarnessError(`arm "${arm.name}": ${arg} does not exist (build or install it first)`);
    }
    if (pin) {
      const ran = spawnSync(pin, ['--version'], { encoding: 'utf8' });
      if (ran.status !== 0) throw new HarnessError(`arm "${arm.name}": ${pin} --version failed: ${ran.error?.message ?? ran.stderr}`);
      versions[arm.name] = `${ran.stdout.trim()} sha256 ${manifest.pins[arm.pin].sha256}`;
    }
  }
  return { ghReal, token: token.stdout.trim(), versions };
}

export async function main(argv) {
  const { options, positional } = parseArgs(argv, { flags: ['--dry-run'], values: ['--pass', '--only', '--arms', '--jobs'] });
  if (positional.length !== 1) throw new HarnessError('pass one round manifest (see --help)');
  const manifest = loadManifest(positional[0]);
  const pass = options.pass === undefined ? undefined : Number(options.pass);
  if (pass !== undefined && !(Number.isSafeInteger(pass) && pass >= 1 && pass <= manifest.passes)) throw new HarnessError(`--pass must be 1 to ${manifest.passes}`);
  const jobs = options.jobs === undefined ? manifest.jobs : Number(options.jobs);
  if (!(Number.isSafeInteger(jobs) && jobs >= 1)) throw new HarnessError('--jobs takes a positive integer');
  const tasks = plan(manifest, { pass, only: options.only && new Set(options.only.split(',')), arms: options.arms && new Set(options.arms.split(',')) });
  if (!tasks.length) throw new HarnessError('the selection matches no run');
  const host = manifest.host;
  const { ghReal, token, versions } = preflight(manifest, tasks, host);
  console.log(`preflight ok: ${JSON.stringify(versions)}`);
  if (options['dry-run']) {
    for (const task of tasks) console.log(`pass ${task.pass} ${task.name}`);
    console.log(`${tasks.length} runs, ${jobs} at a time; no session started`);
    return 0;
  }
  return (await runAll(manifest, tasks, { host, token, ghReal, jobs })) ? 1 : 0;
}

/** Runs `tasks` in order, `jobs` at a time; returns how many produced no result. */
export async function runAll(manifest, tasks, { host, token, ghReal, jobs, log = console.log }) {
  const ghBin = wrapperBin(manifest);
  let failed = 0;
  try {
    await pool(tasks, jobs, task => runOne(manifest, task, { host, token, ghReal, ghBin }), ({ task, status, ok }) => {
      if (!ok) failed += 1;
      const line = `pass ${task.pass} ${task.name} ${status}`;
      log(line);
      fs.appendFileSync(path.join(passPath(manifest, task.pass), 'run.log'), `${line}\n`);
    });
  } finally {
    removeTreeAndEmptyParents(ghBin, manifest.scratch);
  }
  return failed;
}

if (isMain(import.meta.url)) await cli(main, HELP);
