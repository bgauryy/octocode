#!/usr/bin/env node
// Replay gate: runs every distinct Octocode call of a stored round against two pinned binaries, walks every
// top-level next, and fails when evidence the base build returned is not returned by the candidate.
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { walk } from '../../../skills-dev/octocode-dev/scripts/walk.mjs';
import { evidenceOf, missingFrom, rowsOf } from './evidence.mjs';
import {
  CLI, HarnessError, cli, fill, isMain, isRecord, loadManifest, parseArgs, passPath, pool, questions, readTranscript, realGh, removeTree,
  removeTreeAndEmptyParents, runChild, runPath, sessionEnv, sha256File,
} from './lib.mjs';

const HELP = `Usage: node packages/octocode-benchmark/harness/replay.mjs <round manifest> --base <pin> --cand <pin> [options]

<pin> is a pin name of the manifest or a path to an octocode binary. Every distinct Octocode call in the round's
stored transcripts (calls that only carry a cursor are continuations and are reached by walking) runs through
  NODE_ENV=development OCTOCODE_NATIVE_BIN=<pin> node packages/octocode/out/octocode.js <tool> '<json>' --json
on the base and then the candidate, each walked to its last top-level next. The rebuilt rows are reduced to
evidence items (file path + line number + text, match path + line, patch lines, every other value by field
path; the reply envelope is left out) and compared twice:
  reachable    every item the base returns over all pages must be returned by the candidate over all pages;
  first reply  every item in the base's first reply must be in the candidate's first reply.
A call fails when either comparison loses an item that --declare does not cover, or when the candidate's walk
breaks the continuation contract where the base's did not. An item the base itself no longer returns on a second
run is listed as unstable (a live provider answer moved) and does not fail the call.

  --arm A,B        arms whose transcripts supply the calls (default: every MCP arm with a pin)
  --only T,U       only these tools
  --limit N        only the first N distinct calls (in transcript order)
  --jobs N         calls at a time (default: the manifest's "jobs"); calls that create state other calls read
                   (a clone, a materialized tree) run first, one at a time
  --declare FILE   JSON list of intended changes: {"tool"?, "scope"?: "firstReply"|"reachable", "pattern": <regex
                   on the item>, "reason"}; a lost item that matches is reported as declared and does not fail
  --max-pages N    stop a walk after N pages (a pending next is then a violation)
  --out FILE       the full report (default: <campaign>/replay/<base sha>-<cand sha>.json)

Both builds share one fresh Octocode home under .octocode/tmp/<round>/replay/, removed at the end.
Exit codes: 0 no evidence lost; 1 a call lost evidence or broke the contract; 2 bad input.`;

const canonical = value => (Array.isArray(value) ? value.map(canonical) : isRecord(value) ? Object.fromEntries(Object.keys(value).toSorted().map(key => [key, canonical(value[key])])) : value);
const deepStrings = (value, change) => (typeof value === 'string' ? change(value) : Array.isArray(value) ? value.map(item => deepStrings(item, change)) : isRecord(value) ? Object.fromEntries(Object.entries(value).map(([key, item]) => [key, deepStrings(item, change)])) : value);

export const HOME = '{home}';

/**
 * Every distinct tool call in the stored transcripts of `arms`, in transcript order:
 * {tool, input, from: [run names], producer}. A session home in an input becomes `{home}`; `producer` marks a
 * call whose stored reply named a path under the session home that its input did not (it created that state).
 */
export function extractCalls(manifest, arms) {
  const calls = new Map();
  let cursors = 0;
  for (let pass = 1; pass <= manifest.passes; pass += 1) {
    for (const question of questions(manifest)) {
      for (const arm of question.arms.filter(name => arms.includes(name))) {
        const dir = runPath(manifest, pass, question.set, question.id, arm);
        if (!fs.existsSync(path.join(dir, 'transcript.jsonl'))) continue;
        const stored = fs.existsSync(path.join(dir, 'result.json')) ? JSON.parse(fs.readFileSync(path.join(dir, 'result.json'), 'utf8')) : {};
        // A run's session home: recorded by run.mjs; rounds stored before it kept the home inside the run folder.
        const homes = [stored.home, path.join(dir, 'home')].filter(Boolean);
        const neutral = text => homes.reduce((out, home) => out.replaceAll(home, HOME), text);
        for (const step of readTranscript(path.join(dir, 'transcript.jsonl')).steps) {
          for (const block of step.blocks) {
            if (block.type !== 'tool_use' || !block.name.startsWith('mcp__') || !isRecord(block.input)) continue;
            if (block.input.cursor !== undefined) {
              cursors += 1;
              continue;
            }
            const tool = block.name.split('__').at(-1);
            const input = canonical(deepStrings(block.input, neutral));
            const key = `${tool} ${JSON.stringify(input)}`;
            const reply = step.results.find(result => result.id === block.id)?.text ?? '';
            const producer = !key.includes(HOME) && homes.some(home => reply.includes(home));
            const known = calls.get(key);
            if (known) {
              known.from.push(`p${pass}-${question.name}-${arm}`);
              known.producer ||= producer;
            } else calls.set(key, { tool, input, from: [`p${pass}-${question.name}-${arm}`], producer });
          }
        }
      }
    }
  }
  return { calls: [...calls.values()], cursors };
}

/** Runs one CLI call on one binary. */
function runner(bin, env) {
  return args => {
    const ran = spawnSync(process.execPath, [CLI, ...args], { encoding: 'utf8', maxBuffer: 2 ** 31 - 1, stdio: ['ignore', 'pipe', 'pipe'], env: { ...env, NODE_ENV: 'development', OCTOCODE_NATIVE_BIN: bin } });
    if (ran.error) throw new HarnessError(`${CLI} failed to start: ${ran.error.message}`);
    return { status: ran.status, stdout: ran.stdout, stderr: ran.stderr };
  };
}

/**
 * One call on one build, walked to its end: {failed?, pages, chars, first, all, violations, rowErrors, partial}.
 * `first` and `all` are evidence maps of the first reply and of every page.
 */
export function observe({ tool, input, run, schema, maxPages, replace }) {
  let firstReply;
  const capture = args => {
    const ran = run(args);
    if (firstReply === undefined) {
      try {
        firstReply = JSON.parse(ran.stdout);
      } catch {
        firstReply = null;
      }
    }
    return ran;
  };
  const queries = Array.isArray(input.queries) ? input.queries : [];
  let walked;
  try {
    walked = walk({ tool, input, run: capture, schema, maxPages });
  } catch (error) {
    if (error instanceof HarnessError || error.code === undefined) throw error;
    return { failed: `${error.code}: ${error.message}`, pages: 1, chars: 0, first: new Map(), all: new Map(), violations: [], rowErrors: [], partial: [] };
  }
  const whole = walked.violations.find(violation => ['badPage', 'pageError'].includes(violation.code) && violation.page === 1);
  return {
    failed: whole && `${whole.code}: ${whole.message}`,
    pages: walked.pages,
    chars: walked.replyChars,
    first: evidenceOf(rowsOf(firstReply), { queries, replace }),
    all: evidenceOf(walked.results, { queries, replace }),
    violations: walked.violations,
    rowErrors: walked.rowErrors,
    partial: walked.partial,
  };
}

const allowed = (declared, tool, scope) => item => declared.find(rule => (rule.tool === undefined || rule.tool === tool) && (rule.scope === undefined || rule.scope === scope) && new RegExp(rule.pattern).test(item));

/**
 * Compares one call on two builds. `observeOn(side)` observes the call on "base" or "cand".
 * status: same | lost (evidence or contract) | unstable (the base's own answer moved) | baseFailed (nothing to compare).
 */
export function compareCall({ tool, observeOn, declared = [] }) {
  const base = observeOn('base');
  const cand = observeOn('cand');
  const stats = side => ({ pages: side.pages, chars: side.chars, items: side.all.size, firstItems: side.first.size, rowErrors: side.rowErrors.length, partial: side.partial.length, failed: side.failed });
  const out = { status: 'same', base: stats(base), cand: stats(cand), lost: [], firstReplyLost: [], declared: [], unstable: [], contract: [] };
  if (base.failed) return { ...out, status: 'baseFailed' };
  let lost = missingFrom(base.all, cand.all);
  let firstLost = missingFrom(base.first, cand.first);
  if (lost.length || firstLost.length) {
    // A live answer can move between two runs: what the base itself no longer returns is not the candidate's loss.
    const again = observeOn('base');
    const moved = new Set([...missingFrom(base.all, again.all), ...missingFrom(base.first, again.first)]);
    out.unstable = [...moved];
    lost = lost.filter(item => !moved.has(item));
    firstLost = firstLost.filter(item => !moved.has(item));
  }
  for (const [scope, items, into] of [['reachable', lost, out.lost], ['firstReply', firstLost, out.firstReplyLost]]) {
    const rule = allowed(declared, tool, scope);
    for (const item of items) {
      const covered = rule(item);
      if (covered) out.declared.push({ scope, item, reason: covered.reason });
      else into.push(item);
    }
  }
  const known = new Set(base.violations.map(violation => violation.code));
  out.contract = cand.violations.filter(violation => !known.has(violation.code)).map(violation => `${violation.code}: ${violation.message}`);
  if (cand.failed && !out.contract.length) out.contract.push(cand.failed);
  if (out.lost.length || out.firstReplyLost.length || out.contract.length) out.status = 'lost';
  else if (out.unstable.length) out.status = 'unstable';
  return out;
}

/** The worker: one call, both builds, from a job on stdin; prints the comparison. */
async function worker() {
  const chunks = [];
  for await (const chunk of process.stdin) chunks.push(chunk);
  const job = JSON.parse(Buffer.concat(chunks).toString('utf8'));
  const input = deepStrings(job.input, text => text.replaceAll(HOME, job.home));
  const result = compareCall({
    tool: job.tool,
    declared: job.declared,
    observeOn: side => observe({ tool: job.tool, input, run: runner(job.bins[side], job.env), schema: job.schemas[side], maxPages: job.maxPages, replace: { [job.home]: HOME } }),
  });
  process.stdout.write(JSON.stringify(result));
}

function resolveBin(manifest, value, flag) {
  if (!value) throw new HarnessError(`${flag} needs a pin name of the manifest or a binary path`);
  const pin = manifest.pins[value];
  const file = pin ? pin.path : path.resolve(value);
  if (!fs.existsSync(file)) throw new HarnessError(`${flag}: ${file} does not exist`);
  const sha256 = sha256File(file);
  if (pin && pin.sha256 !== sha256) throw new HarnessError(`${flag}: ${file} has SHA-256 ${sha256}, the manifest records ${pin.sha256}`);
  return { path: file, sha256 };
}

export async function main(argv) {
  if (argv[0] === '--worker') {
    await worker();
    return 0;
  }
  const { options, positional } = parseArgs(argv, { values: ['--base', '--cand', '--arm', '--only', '--limit', '--jobs', '--declare', '--max-pages', '--out'] });
  if (positional.length !== 1) throw new HarnessError('pass one round manifest (see --help)');
  const manifest = loadManifest(positional[0]);
  const bins = { base: resolveBin(manifest, options.base, '--base'), cand: resolveBin(manifest, options.cand, '--cand') };
  if (!fs.existsSync(CLI)) throw new HarnessError(`${CLI} is not built; run \`dev build:dev\``);
  const arms = options.arm ? options.arm.split(',') : Object.values(manifest.arms).filter(arm => arm.kind === 'mcp' && arm.pin !== undefined).map(arm => arm.name);
  for (const arm of arms) if (manifest.arms[arm]?.kind !== 'mcp') throw new HarnessError(`--arm ${arm} is not an MCP arm of the manifest`);
  if (!arms.length) throw new HarnessError('no MCP arm with a pin supplies calls; name one with --arm');
  const number = (name, fallback) => {
    if (options[name] === undefined) return fallback;
    const value = Number(options[name]);
    if (!(Number.isSafeInteger(value) && value >= 1)) throw new HarnessError(`--${name} takes a positive integer`);
    return value;
  };
  const jobs = number('jobs', manifest.jobs);
  const maxPages = number('max-pages', undefined);
  const limit = number('limit', undefined);
  let declared = [];
  if (options.declare) {
    declared = JSON.parse(fs.readFileSync(path.resolve(options.declare), 'utf8'));
    if (!Array.isArray(declared) || !declared.every(rule => isRecord(rule) && typeof rule.pattern === 'string' && typeof rule.reason === 'string' && rule.reason)) throw new HarnessError('--declare: every entry needs "pattern" and "reason"');
  }
  const extracted = extractCalls(manifest, arms);
  const tools = options.only && new Set(options.only.split(','));
  let calls = extracted.calls.filter(call => !tools || tools.has(call.tool));
  if (limit) calls = calls.slice(0, limit);
  if (!calls.length) throw new HarnessError('the selection matches no stored call');

  const scratch = path.join(manifest.scratch, 'replay');
  const home = path.join(scratch, 'home');
  removeTree(scratch);
  fs.mkdirSync(home, { recursive: true });
  const token = spawnSync(realGh(), ['auth', 'token'], { encoding: 'utf8' }).stdout.trim();
  const { NODE_ENV: _mode, OCTOCODE_NATIVE_BIN: _bin, ...server } = fill(manifest.arms[arms[0]].server.env ?? {}, manifest, { home, pin: bins.base.path });
  const env = { ...sessionEnv(), ...(token ? { GITHUB_TOKEN: token } : {}), ...server };
  const results = [];
  try {
    const schemas = { base: {}, cand: {} };
    for (const side of ['base', 'cand']) {
      const run = runner(bins[side].path, env);
      for (const tool of new Set(calls.map(call => call.tool))) {
        const ran = run(['schema', tool, '--view', 'output', '--json']);
        try {
          schemas[side][tool] = JSON.parse(ran.stdout);
        } catch {
          throw new HarnessError(`${side}: schema ${tool} --view output failed (exit ${ran.status}): ${ran.stderr.trim()}`);
        }
      }
    }
    const one = async call => {
      const job = { tool: call.tool, input: call.input, home, env, declared, maxPages, bins: { base: bins.base.path, cand: bins.cand.path }, schemas: { base: schemas.base[call.tool], cand: schemas.cand[call.tool] } };
      const ran = await runChild(process.execPath, [path.join(import.meta.dirname, 'replay.mjs'), '--worker'], { input: JSON.stringify(job) });
      let result;
      try {
        result = JSON.parse(ran.stdout);
      } catch {
        result = { status: 'lost', lost: [], firstReplyLost: [], declared: [], unstable: [], contract: [`the comparison did not finish (exit ${ran.code}): ${ran.stderr.trim().split('\n').at(-1) ?? ''}`] };
      }
      return { call, ...result };
    };
    const report = result => {
      results.push(result);
      if (result.status !== 'same') console.log(`${result.status.toUpperCase()} ${result.call.tool} ${JSON.stringify(result.call.input)}`);
    };
    await pool(calls.filter(call => call.producer), 1, one, report);
    await pool(calls.filter(call => !call.producer), jobs, one, report);
  } finally {
    removeTreeAndEmptyParents(scratch, manifest.scratch);
  }

  const by = status => results.filter(result => result.status === status);
  const total = (side, key) => results.reduce((sum, result) => sum + (result[side]?.[key] ?? 0), 0);
  const summary = {
    round: manifest.round,
    base: bins.base,
    cand: bins.cand,
    arms,
    calls: results.length,
    cursorCallsInTranscripts: extracted.cursors,
    same: by('same').length,
    unstable: by('unstable').length,
    baseFailed: by('baseFailed').length,
    lost: by('lost').length,
    declaredItems: results.reduce((sum, result) => sum + result.declared.length, 0),
    totals: {
      base: { pages: total('base', 'pages'), chars: total('base', 'chars'), items: total('base', 'items'), firstItems: total('base', 'firstItems'), rowErrors: total('base', 'rowErrors') },
      cand: { pages: total('cand', 'pages'), chars: total('cand', 'chars'), items: total('cand', 'items'), firstItems: total('cand', 'firstItems'), rowErrors: total('cand', 'rowErrors') },
    },
  };
  const out = options.out ? path.resolve(options.out) : path.join(manifest.campaign, 'replay', `${bins.base.sha256.slice(0, 12)}-${bins.cand.sha256.slice(0, 12)}.json`);
  fs.mkdirSync(path.dirname(out), { recursive: true });
  fs.writeFileSync(out, `${JSON.stringify({ ...summary, results }, null, 1)}\n`);
  for (const result of by('lost')) {
    console.log(`\nFAILED ${result.call.tool} ${JSON.stringify(result.call.input)}\n  first seen in ${result.call.from[0]}`);
    for (const [title, items] of [['not reachable in the candidate', result.lost], ['missing from the candidate\'s first reply', result.firstReplyLost], ['contract', result.contract]]) {
      if (!items.length) continue;
      console.log(`  ${title} (${items.length}):`);
      for (const item of items.slice(0, 10)) console.log(`    ${item}`);
      if (items.length > 10) console.log(`    … ${items.length - 10} more in ${out}`);
    }
  }
  console.log(`\nreplay ${manifest.round}: ${summary.calls} calls: ${summary.same} same, ${summary.lost} lost evidence, ${summary.unstable} unstable, ${summary.baseFailed} failed on the base (nothing to compare); ${summary.declaredItems} declared items`);
  console.log(`base ${summary.totals.base.pages} pages, ${summary.totals.base.chars.toLocaleString('en-US')} characters, ${summary.totals.base.items.toLocaleString('en-US')} evidence items (${summary.totals.base.firstItems.toLocaleString('en-US')} in first replies); candidate ${summary.totals.cand.pages} pages, ${summary.totals.cand.chars.toLocaleString('en-US')} characters, ${summary.totals.cand.items.toLocaleString('en-US')} items (${summary.totals.cand.firstItems.toLocaleString('en-US')})`);
  console.log(`report: ${out}`);
  return summary.lost ? 1 : 0;
}

if (isMain(import.meta.url)) await cli(main, HELP);
