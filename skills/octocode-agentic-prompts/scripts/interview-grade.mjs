#!/usr/bin/env node
// Replays agentic-interview cold calls through a runner command and classifies each outcome.

import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';

const HELP = `Replay interview cold calls (<dir>/<GROUP>-calls.jsonl) and grade each outcome.

Usage:
  node scripts/interview-grade.mjs <round-dir> --cmd '<runner with {tool} and {input}>' [options]

Options:
  --cmd <template>    Runner; {tool} and {input} are replaced with shell-quoted values
  --cwd <dir>         Working directory for the runner (default: current)
  --reject <regex>    Output that means the input was rejected (schema/validation)
  --error <regex>     Output that means the call failed; checked before --empty
                      (with no pattern match, a non-zero exit counts as error)
  --empty <regex>     Output that means a valid call with no results
  --exclude <regex>   GROUP:SCENARIO ids that are expected to fail (e.g. "G1:S8|G6:S5")
  --timeout <ms>      Per-call timeout (default 90000)
  --json              Print the summary as JSON

Outcomes: ok, empty, rejected, error, crash, placeholder, excluded, badLine.
Valid cold rate = (ok + empty) / (all - placeholder - excluded - badLine).
Writes <round-dir>/graded.json. Requires at least one gradable call; exits 2 when none
remain or a line is malformed. The runner template must contain {tool} and {input}.`;

function parseArgs(argv) {
  const args = { _: [], timeout: 90000 };
  const values = new Set(['--cmd', '--cwd', '--reject', '--error', '--empty', '--exclude', '--timeout']);
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === '--help' || a === '-h') args.help = true;
    else if (a === '--json') args.json = true;
    else if (values.has(a)) {
      const value = argv[++i];
      if (value === undefined || value.startsWith('--')) throw new Error(`${a} requires a value`);
      args[a.slice(2)] = a === '--timeout' ? Number(value) : value;
    }
    else if (a.startsWith('--')) throw new Error(`Unknown option: ${a}`);
    else args._.push(a);
  }
  return args;
}

const quote = (value) => `'${String(value).replaceAll("'", `'\\''`)}'`;
const regex = (source) => (source ? new RegExp(source) : null);

function classify(result, patterns) {
  if (result.error || result.status === null) return 'crash';
  const text = `${result.stdout}\n${result.stderr}`;
  // Patterns win over the exit code: runners differ on what a non-zero exit means.
  if (patterns.reject?.test(text)) return 'rejected';
  if (patterns.error?.test(text)) return 'error';
  if (patterns.empty?.test(text)) return 'empty';
  return result.status === 0 ? 'ok' : 'error';
}

function parseCall(line, index, group) {
  let call;
  try { call = JSON.parse(line); }
  catch { return { error: { group, line: index + 1, outcome: 'badLine' } }; }
  if (!call || typeof call !== 'object' || Array.isArray(call)) return { error: { group, line: index + 1, outcome: 'badLine' } };
  return { call };
}

const validCall = (call) => typeof call.tool === 'string' && Boolean(call.tool.trim()) &&
  Boolean(call.input) && typeof call.input === 'object' && !Array.isArray(call.input);

function gradeLine(line, index, group, settings) {
  const parsed = parseCall(line, index, group);
  if (parsed.error) return parsed.error;
  const { call } = parsed;
  const { args, patterns, exclude } = settings;
  const base = { group, scenario: call.scenario, step: call.step, tool: call.tool };
  const input = JSON.stringify(call.input ?? null);
  if (!validCall(call)) return { ...base, outcome: 'badLine' };
  if (/<[^<>]+>/.test(input)) return { ...base, outcome: 'placeholder' };
  if (exclude?.test(`${group}:${call.scenario}`)) return { ...base, outcome: 'excluded' };
  const command = args.cmd.replace(/\{(tool|input)\}/g, (_, key) => quote(key === 'tool' ? call.tool : input));
  const result = spawnSync('sh', ['-c', command], {
    cwd: args.cwd ?? process.cwd(), encoding: 'utf8', timeout: args.timeout, maxBuffer: 64 * 1024 * 1024,
  });
  const outcome = classify(result, patterns);
  const detail = outcome === 'ok' || outcome === 'empty' ? undefined : `${result.stdout}${result.stderr}`.trim().slice(0, 240);
  return { ...base, outcome, bytes: result.stdout?.length ?? 0, ...(detail ? { detail } : {}) };
}

function gradeFiles(dir, settings) {
  const files = fs.readdirSync(dir).filter((f) => f.endsWith('-calls.jsonl')).toSorted();
  if (!files.length) throw new Error(`No *-calls.jsonl files in ${dir}`);
  const rows = [];
  for (const file of files) {
    const group = file.slice(0, file.indexOf('-'));
    const lines = fs.readFileSync(path.join(dir, file), 'utf8').split('\n');
    lines.forEach((line, index) => { if (line.trim()) rows.push(gradeLine(line, index, group, settings)); });
  }
  return rows;
}

function reportRows(rows, args) {
  const summary = {};
  for (const row of rows) {
    const g = (summary[row.group] ??= { calls: 0 });
    g.calls++;
    g[row.outcome] = (g[row.outcome] ?? 0) + 1;
  }
  const count = (o) => rows.filter((r) => r.outcome === o).length;
  const gradable = rows.length - count('placeholder') - count('excluded') - count('badLine');
  if (!gradable || count('badLine')) process.exitCode = 2;
  const valid = count('ok') + count('empty');
  const total = { gradable, valid, validRate: gradable ? +(valid / gradable).toFixed(3) : null, rejected: count('rejected') };
  const failures = rows.filter((r) => ['rejected', 'error', 'crash'].includes(r.outcome));
  if (args.json) {
    console.log(JSON.stringify({ total, groups: summary, failures }, null, 1));
    return;
  }
  console.log(`valid cold ${valid}/${gradable}${gradable ? ` (${Math.round((100 * valid) / gradable)}%)` : ''}, rejected ${total.rejected}`);
  for (const [group, s] of Object.entries(summary)) console.log(`${group}: ${JSON.stringify(s)}`);
  for (const f of failures) console.log(`${f.group} ${f.scenario} step ${f.step} ${f.tool}: ${f.outcome} :: ${f.detail ?? ''}`);
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  const dir = args._[0];
  if (args.help || !dir || !args.cmd) {
    console.log(HELP);
    process.exitCode = args.help ? 0 : 1;
    return;
  }
  if (args._.length !== 1 || !args.cmd.includes('{tool}') || !args.cmd.includes('{input}') || !Number.isSafeInteger(args.timeout) || args.timeout < 1) {
    throw new Error('Use one round directory, a --cmd containing {tool} and {input}, and a positive integer --timeout');
  }
  const patterns = { reject: regex(args.reject), error: regex(args.error), empty: regex(args.empty) };
  const exclude = regex(args.exclude);
  const rows = gradeFiles(dir, { args, patterns, exclude });
  fs.writeFileSync(path.join(dir, 'graded.json'), `${JSON.stringify(rows, null, 1)}\n`);
  reportRows(rows, args);
}

main();
