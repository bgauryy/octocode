#!/usr/bin/env node
// Tallies skill-activation runs: the first Skill call per host stream-json run, classified against the expected skill.

import { readdirSync, readFileSync, statSync } from 'node:fs';
import { basename, dirname, join, resolve } from 'node:path';
import { fail, errorMessage } from './lib/fail.mjs';
import { isMainModule } from './lib/entrypoint.mjs';

const HELP = `Classify skill-activation runs and count outcomes per request set and arm.

Usage:
  node scripts/activation-tally.mjs <runs-dir | run.jsonl>... --expected <requests.jsonl>

Inputs:
  --expected   JSONL, one request per line: {"id":"r1","set":"tuning","expected":"octocode-debugging"}
               expected is the skill that must load first, or null for a no-skill request; other fields are ignored.
  runs         Host stream-json files (claude -p ... --output-format stream-json --verbose), one run per file,
               named <arm>/<id>.jsonl or <arm>/<id>.<repeat>.jsonl. The parent folder names the arm. Directories
               are searched recursively for *.jsonl; the --expected file is skipped.

Reading a run: the first top-level tool_use named "Skill" (parent_tool_use_id empty) gives firstSkill = input.skill.
A loaded "plugin:skill" matches an expected "skill". The run is complete when it has a result event with is_error false.
Outcomes: correct (expected skill first), wrongRoute (another skill first), miss (complete run, no skill, one expected),
correctRejection (complete run, no skill, none expected), falseTrigger (a skill for a no-skill request),
runError (no skill call and the run is incomplete or failed: not a miss).

Output (stdout JSON): runs[] {file, arm, id, repeat, set, expected, firstSkill, outcome, hostVersion, model, costUsd,
badLines}; sets[] per set and arm {set, arm, runs, correct, miss, wrongRoute, correctRejection, falseTrigger, runError,
winners (wrong-route and false-trigger skill counts), costUsd summed over costRuns runs that report it}; missing[] {arm, id} requests with no run in an arm.
Exit codes: 0 every request has a run in every arm and no runError; 1 tally printed but coverage is incomplete
(missing or runError); 2 invalid input (unreadable or malformed requests, a run file that names no request, no runs).`;

const isObject = (value) => Boolean(value) && typeof value === 'object' && !Array.isArray(value);

export function parseRequests(text, file = 'requests') {
  const requests = new Map();
  text.split('\n').forEach((line, index) => {
    if (!line.trim()) return;
    let row;
    try { row = JSON.parse(line); }
    catch { throw new Error(`${file}:${index + 1}: not JSON`); }
    if (!isObject(row) || typeof row.id !== 'string' || !row.id || typeof row.set !== 'string' || !row.set ||
      !(row.expected === null || (typeof row.expected === 'string' && row.expected))) {
      throw new Error(`${file}:${index + 1}: need {id, set, expected: skill name or null}`);
    }
    if (requests.has(row.id)) throw new Error(`${file}:${index + 1}: duplicate id ${row.id}`);
    requests.set(row.id, { id: row.id, set: row.set, expected: row.expected });
  });
  if (!requests.size) throw new Error(`${file}: no requests`);
  return requests;
}

/** Reads one stream-json run: first top-level Skill call, completion and run metadata. */
export function readRun(text) {
  const run = { firstSkill: null, skillCall: false, complete: false, hostVersion: null, model: null, costUsd: null, badLines: 0 };
  for (const line of text.split('\n')) {
    if (!line.trim()) continue;
    let event;
    try { event = JSON.parse(line); }
    catch { run.badLines++; continue; }
    if (!isObject(event)) { run.badLines++; continue; }
    if (event.type === 'system' && event.subtype === 'init') {
      run.hostVersion ??= event.claude_code_version ?? event.version ?? null;
      run.model ??= event.model ?? null;
    } else if (event.type === 'assistant' && !run.skillCall && !event.parent_tool_use_id) {
      const call = (event.message?.content ?? []).find((block) => block?.type === 'tool_use' && block.name === 'Skill');
      if (call) {
        run.skillCall = true;
        run.firstSkill = typeof call.input?.skill === 'string' ? call.input.skill : null;
      }
    } else if (event.type === 'result') {
      run.complete = event.is_error === false;
      run.costUsd = typeof event.total_cost_usd === 'number' ? event.total_cost_usd : null;
    }
  }
  return run;
}

const matches = (loaded, expected) => loaded === expected || (!expected.includes(':') && loaded.split(':').at(-1) === expected);

export function outcome(run, expected) {
  if (run.skillCall) {
    if (expected === null) return 'falseTrigger';
    return run.firstSkill !== null && matches(run.firstSkill, expected) ? 'correct' : 'wrongRoute';
  }
  if (!run.complete) return 'runError';
  return expected === null ? 'correctRejection' : 'miss';
}

function runFiles(paths, skip) {
  const files = [];
  const walk = (path) => {
    if (statSync(path).isDirectory()) {
      for (const entry of readdirSync(path).toSorted()) walk(join(path, entry));
    } else if (path.endsWith('.jsonl') && resolve(path) !== skip) files.push(path);
  };
  for (const path of paths) {
    if (!statSync(path).isDirectory() && !path.endsWith('.jsonl')) throw new Error(`${path}: run files end in .jsonl`);
    walk(path);
  }
  return files;
}

function identify(file, requests) {
  const stem = basename(file, '.jsonl');
  if (requests.has(stem)) return { id: stem, repeat: null };
  const repeated = /^(.*)\.(\d+)$/.exec(stem);
  if (repeated && requests.has(repeated[1])) return { id: repeated[1], repeat: Number(repeated[2]) };
  throw new Error(`${file}: file name names no request id in --expected`);
}

const OUTCOMES = ['correct', 'miss', 'wrongRoute', 'correctRejection', 'falseTrigger', 'runError'];

export function tally(requests, files) {
  const runs = files.map(({ file, text }) => {
    const { id, repeat } = identify(file, requests);
    const request = requests.get(id);
    const run = readRun(text);
    return {
      file, arm: basename(dirname(resolve(file))), id, repeat, set: request.set, expected: request.expected,
      firstSkill: run.firstSkill, outcome: outcome(run, request.expected), hostVersion: run.hostVersion,
      model: run.model, costUsd: run.costUsd, badLines: run.badLines,
    };
  });
  const groups = new Map();
  for (const run of runs) {
    const key = `${run.set}\u0000${run.arm}`;
    if (!groups.has(key)) groups.set(key, { set: run.set, arm: run.arm, runs: 0, ...Object.fromEntries(OUTCOMES.map((o) => [o, 0])), winners: {}, costUsd: 0, costRuns: 0 });
    const group = groups.get(key);
    group.runs++;
    group[run.outcome]++;
    if (run.outcome === 'wrongRoute' || run.outcome === 'falseTrigger') {
      const winner = run.firstSkill ?? '(Skill call without input.skill)';
      group.winners[winner] = (group.winners[winner] ?? 0) + 1;
    }
    if (run.costUsd !== null) { group.costUsd = Number((group.costUsd + run.costUsd).toFixed(6)); group.costRuns++; }
  }
  const arms = [...new Set(runs.map((run) => run.arm))].toSorted();
  const missing = arms.flatMap((arm) => [...requests.keys()]
    .filter((id) => !runs.some((run) => run.arm === arm && run.id === id)).map((id) => ({ arm, id })));
  const sets = [...groups.values()].toSorted((a, b) => a.set.localeCompare(b.set) || a.arm.localeCompare(b.arm));
  return { ok: true, requests: requests.size, arms, sets, missing, runs };
}

function main(argv) {
  if (!argv.length || argv.includes('--help') || argv.includes('-h')) {
    console.log(HELP);
    if (!argv.length) process.exitCode = 2;
    return;
  }
  const paths = [];
  let expectedFile;
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === '--expected') {
      expectedFile = argv[++i];
      if (!expectedFile || expectedFile.startsWith('--')) fail('invalidArguments', '--expected requires a file', 2);
    } else if (argv[i].startsWith('--')) fail('invalidArguments', `Unknown option: ${argv[i]} (see --help)`, 2);
    else paths.push(argv[i]);
  }
  if (!expectedFile || !paths.length) fail('invalidArguments', 'give run files or directories and --expected <requests.jsonl>', 2);
  let result;
  try {
    const requests = parseRequests(readFileSync(expectedFile, 'utf8'), expectedFile);
    const files = runFiles(paths, resolve(expectedFile)).map((file) => ({ file, text: readFileSync(file, 'utf8') }));
    if (!files.length) throw new Error('no *.jsonl run files found');
    result = tally(requests, files);
  } catch (error) {
    fail('invalidInput', errorMessage(error), 2);
  }
  console.log(JSON.stringify(result, null, 1));
  const incomplete = result.missing.length > 0 || result.runs.some((run) => run.outcome === 'runError');
  process.exitCode = incomplete ? 1 : 0;
}

if (isMainModule(import.meta.url)) main(process.argv.slice(2));
