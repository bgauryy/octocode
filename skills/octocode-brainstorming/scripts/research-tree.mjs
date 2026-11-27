#!/usr/bin/env node
/**
 * Checks a research tree (SKILL.md steps 4 and 7): one plan row per claim, grouped by sub-question and worker.
 * With verified ledgers it joins each planned claim to its verified rows and names the claims still open or
 * one-sided, the ones the next round deepens. Reads files; changes nothing.
 */
import { readFileSync } from 'node:fs';
import { errorMessage, fail } from './lib/fail.mjs';
import { isMainModule } from './lib/entrypoint.mjs';

export const SURFACES = ['web', 'news', 'papers', 'github'];
export const DIRECTIONS = ['supports', 'refutes'];
// Ledger statuses that count as evidence (references/verification.md).
const COUNTED = new Set(['verified', 'verified-manual']);

const USAGE = `Usage: research-tree.mjs --plan <plan.jsonl> [--verified <ledger.verified.jsonl>]...

Checks the research plan: one JSON line per claim, a leaf of decision → sub-question → claim → queries.
  {"id":"C1","question":"<sub-question>","claim":"<one falsifiable sentence>","rival":"<strongest rival>",
   "refutedBy":"<observation that would refute it>","option":"<option id, optional>","worker":"<worker, optional>",
   "queries":[{"direction":"supports|refutes","surface":"web|news|papers|github","text":"<query>"}]}
Every claim needs a rival, a refuting observation and at least one query in each direction.

Prints JSON on stdout:
  questions[]        each sub-question with its claim ids, in plan order
  workers[]          each worker (or "unassigned") with its claim ids and surfaces: one brief per worker
  claims             planned claims
With --verified (verify-claims.mjs --out files; repeatable), joining rows whose "decides" is a claim id:
  coverage[]         per claim: verified rows, supports, refutes, distinct sources, claimStatus values, unverified
                     rows, and state: open (no verified row), supportOnly, refuteOnly or bothSides
  states             claims per state
  deepen[]           claims open or one-sided: run another round for them, or record why the other side is empty
  conflicting[]      claims whose rows carry different claimStatus values
  unplanned[]        decides ids in the ledgers that the plan does not name
Exit 0 with a result, 1 when the plan holds no claim, 2 for invalid input (the message names file:line).`;

class InvalidPlan extends Error {}

/** Parses JSONL files into objects with their file:line; a malformed line is invalid input. */
export function readJsonl(path, read = (p) => readFileSync(p, 'utf8')) {
  let text;
  try { text = read(path); } catch (error) { throw new InvalidPlan(`cannot read ${path}: ${errorMessage(error)}`); }
  const rows = [];
  text.split('\n').forEach((line, index) => {
    if (!line.trim()) return;
    let row;
    try { row = JSON.parse(line); } catch { throw new InvalidPlan(`${path}:${index + 1}: not a JSON object`); }
    if (!row || typeof row !== 'object' || Array.isArray(row)) throw new InvalidPlan(`${path}:${index + 1}: not a JSON object`);
    rows.push({ ...row, at: `${path}:${index + 1}` });
  });
  return rows;
}

const text = (v) => typeof v === 'string' && v.trim().length > 0;

function validatePlan(rows) {
  const ids = new Map();
  for (const row of rows) {
    for (const field of ['id', 'question', 'claim', 'rival', 'refutedBy']) {
      if (!text(row[field])) throw new InvalidPlan(`${row.at}: "${field}" must be a non-empty string`);
    }
    for (const field of ['option', 'worker']) {
      if (row[field] != null && !text(row[field])) throw new InvalidPlan(`${row.at}: "${field}" must be a non-empty string when set`);
    }
    if (ids.has(row.id)) throw new InvalidPlan(`${row.at}: id ${row.id} repeats ${ids.get(row.id)}`);
    ids.set(row.id, row.at);
    if (!Array.isArray(row.queries) || !row.queries.length) throw new InvalidPlan(`${row.at}: "queries" must be a non-empty array`);
    row.queries.forEach((q, i) => {
      if (!q || !DIRECTIONS.includes(q.direction)) throw new InvalidPlan(`${row.at}: queries[${i}].direction must be one of ${DIRECTIONS.join(', ')}`);
      if (!SURFACES.includes(q.surface)) throw new InvalidPlan(`${row.at}: queries[${i}].surface must be one of ${SURFACES.join(', ')}`);
      if (!text(q.text)) throw new InvalidPlan(`${row.at}: queries[${i}].text must be a non-empty string`);
    });
    for (const direction of DIRECTIONS) {
      if (!row.queries.some((q) => q.direction === direction)) throw new InvalidPlan(`${row.at}: claim ${row.id} has no "${direction}" query; plan both directions`);
    }
  }
}

/** Groups a validated plan into sub-questions and worker assignments. */
export function planTree(plan) {
  validatePlan(plan);
  const questions = new Map();
  const workers = new Map();
  for (const row of plan) {
    const q = questions.get(row.question) || { question: row.question, claims: [] };
    q.claims.push(row.id);
    questions.set(row.question, q);
    const name = row.worker || 'unassigned';
    const w = workers.get(name) || { worker: name, claims: [], surfaces: [] };
    w.claims.push(row.id);
    for (const { surface } of row.queries) if (!w.surfaces.includes(surface)) w.surfaces.push(surface);
    workers.set(name, w);
  }
  return { claims: plan.length, questions: [...questions.values()], workers: [...workers.values()] };
}

const stateOf = (supports, refutes) => (supports && refutes ? 'bothSides' : supports ? 'supportOnly' : refutes ? 'refuteOnly' : 'open');

/** Joins verified ledger rows to the planned claims by their `decides` id. */
export function coverage(plan, ledgerRows) {
  const planned = new Set(plan.map((row) => row.id));
  const byClaim = new Map(plan.map((row) => [row.id, []]));
  const unplanned = new Map();
  for (const row of ledgerRows) {
    if (!text(row.decides)) continue;
    if (planned.has(row.decides)) byClaim.get(row.decides).push(row);
    else unplanned.set(row.decides, (unplanned.get(row.decides) || 0) + 1);
  }
  const rows = plan.map(({ id }) => {
    const all = byClaim.get(id);
    const counted = all.filter((r) => COUNTED.has(r.verification?.status));
    const supports = counted.filter((r) => r.direction === 'supports').length;
    const refutes = counted.filter((r) => r.direction === 'refutes').length;
    const statuses = [...new Set(counted.map((r) => r.claimStatus).filter(Boolean))];
    const sources = new Set(counted.map((r) => r.verification?.finalUrl || r.url)).size;
    return { id, verified: counted.length, supports, refutes, sources, claimStatus: statuses, unverified: all.length - counted.length, state: stateOf(supports, refutes) };
  });
  const states = Object.fromEntries(['open', 'supportOnly', 'refuteOnly', 'bothSides'].map((s) => [s, rows.filter((r) => r.state === s).length]));
  return {
    coverage: rows,
    states,
    deepen: rows.filter((r) => r.state !== 'bothSides').map((r) => ({ id: r.id, state: r.state })),
    conflicting: rows.filter((r) => r.claimStatus.length > 1).map((r) => ({ id: r.id, claimStatus: r.claimStatus })),
    unplanned: [...unplanned].map(([decides, count]) => ({ decides, rows: count })),
  };
}

function parseArgs(argv) {
  const opts = { plan: null, verified: [] };
  for (let i = 0; i < argv.length; i += 1) {
    const flag = argv[i];
    if (flag === '--help' || flag === '-h') { console.log(USAGE); process.exit(0); }
    if (flag !== '--plan' && flag !== '--verified') fail('invalidArguments', `Unknown option: ${flag} (see --help)`, 2);
    const value = argv[i + 1];
    if (value == null || value.startsWith('--')) fail('invalidArguments', `missing value for ${flag}`, 2);
    i += 1;
    if (flag === '--plan') {
      if (opts.plan) fail('invalidArguments', 'give one --plan file', 2);
      opts.plan = value;
    } else opts.verified.push(value);
  }
  if (!opts.plan) fail('invalidArguments', '--plan is required (see --help)', 2);
  return opts;
}

if (isMainModule(import.meta.url)) {
  const opts = parseArgs(process.argv.slice(2));
  let result;
  try {
    const plan = readJsonl(opts.plan);
    if (!plan.length) fail('noClaims', `no claim rows in ${opts.plan}`, 1);
    result = planTree(plan);
    if (opts.verified.length) Object.assign(result, coverage(plan, opts.verified.flatMap((path) => readJsonl(path))));
  } catch (error) {
    if (error instanceof InvalidPlan) fail('invalidPlan', error.message, 2);
    throw error;
  }
  console.log(JSON.stringify({ ok: true, ...result }, null, 2));
}
