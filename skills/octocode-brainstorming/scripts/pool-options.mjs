#!/usr/bin/env node
/**
 * Pools brainstorming options from independent passes (SKILL.md step 3): counts the distinct options each pass
 * added, groups the duplicates the agent marked with `sameAs`, and lists unmarked pairs that share most of their
 * words so the agent can judge them. Judging whether two options mean the same is the agent's; counting is this
 * script's. Reads files; changes nothing.
 */
import { readFileSync } from 'node:fs';
import { errorMessage, fail } from './lib/fail.mjs';
import { isMainModule } from './lib/entrypoint.mjs';

export const ORIGINS = ['user', 'yours', 'found'];
// A pair at or above this word overlap (Jaccard) is listed for review; it is never merged automatically.
export const DEFAULT_MIN_SIMILARITY = 0.5;
// Words too common to tell two options apart.
const STOP_WORDS = new Set(['the', 'and', 'for', 'with', 'into', 'from', 'that', 'this', 'then', 'than', 'are', 'its', 'our', 'your', 'use', 'using', 'via', 'per', 'each', 'all', 'any', 'not', 'but', 'can', 'will', 'one']);

const USAGE = `Usage: pool-options.mjs --options <file.jsonl> [--options <file.jsonl>]... [--min-similarity <0-1>]

Pools the options of independent brainstorming passes. Each line of a file is one option:
  {"id":"A1","pass":"<pass name>","origin":"user|yours|found","option":"<short name>","claim":"<testable claim>","sameAs":"<id>"}
id, pass, origin and option are required; claim is optional; sameAs marks this option as a duplicate of another id
(your judgment after reading both). Rows are read in file and line order; a pass is new when it first appears.

Prints JSON on stdout:
  options, distinct      rows read and distinct options after sameAs merges
  passes[]               per pass in order: options, new (distinct options it introduced), repeats
  saturated              true when there are two or more passes and the last one added no new distinct option
  clusters[]             each distinct option: id (its first row), option, members, passes, origins
  candidates[]           unmerged pairs whose words overlap at least --min-similarity (default ${DEFAULT_MIN_SIMILARITY}),
                         highest first: read both, then add sameAs or leave them apart, and rerun
  origins                distinct options that include each origin (user, yours, found)
Resolve candidates before reading saturated: it counts only sameAs merges.
Exit 0 with a result, 1 when the files hold no option, 2 for invalid input (the message names file:line).`;

/** The words of a text that can tell two options apart: lowercased, unaccented, without punctuation or stop words. */
export function words(text) {
  const plain = String(text || '').toLowerCase().normalize('NFKD').replace(/\p{M}/gu, '').replace(/[^\p{L}\p{N}]+/gu, ' ');
  return new Set(plain.split(' ').filter((w) => w.length > 2 && !STOP_WORDS.has(w)));
}

/** Jaccard overlap of two word sets: shared words over all words, 0 when both are empty. */
export function similarity(a, b) {
  let shared = 0;
  for (const w of a) if (b.has(w)) shared += 1;
  const all = a.size + b.size - shared;
  return all ? shared / all : 0;
}

class InvalidOptions extends Error {}

/** Parses JSONL option files into rows with their file:line, rejecting a malformed row with its location. */
export function readOptionFiles(paths, read = (p) => readFileSync(p, 'utf8')) {
  const rows = [];
  for (const path of paths) {
    let text;
    try { text = read(path); } catch (error) { throw new InvalidOptions(`cannot read ${path}: ${errorMessage(error)}`); }
    text.split('\n').forEach((line, index) => {
      if (!line.trim()) return;
      let row;
      try { row = JSON.parse(line); } catch { throw new InvalidOptions(`${path}:${index + 1}: not a JSON object`); }
      if (!row || typeof row !== 'object' || Array.isArray(row)) throw new InvalidOptions(`${path}:${index + 1}: not a JSON object`);
      rows.push({ ...row, at: `${path}:${index + 1}` });
    });
  }
  return rows;
}

function validate(rows) {
  const ids = new Map();
  for (const row of rows) {
    for (const field of ['id', 'pass', 'option']) {
      if (typeof row[field] !== 'string' || !row[field].trim()) throw new InvalidOptions(`${row.at}: "${field}" must be a non-empty string`);
    }
    if (!ORIGINS.includes(row.origin)) throw new InvalidOptions(`${row.at}: "origin" must be one of ${ORIGINS.join(', ')}`);
    if (ids.has(row.id)) throw new InvalidOptions(`${row.at}: id ${row.id} repeats ${ids.get(row.id)}`);
    ids.set(row.id, row.at);
  }
  for (const row of rows) {
    if (row.sameAs == null) continue;
    if (!ids.has(row.sameAs)) throw new InvalidOptions(`${row.at}: sameAs ${row.sameAs} is not an id in the files`);
    if (row.sameAs === row.id) throw new InvalidOptions(`${row.at}: sameAs names the row itself`);
  }
}

/** Pools validated option rows: distinct options after sameAs merges, per-pass novelty and review candidates. */
export function poolOptions(rows, { minSimilarity = DEFAULT_MIN_SIMILARITY } = {}) {
  validate(rows);
  const order = new Map(rows.map((row, index) => [row.id, index]));
  // Union-find over sameAs links; the root of a cluster is its earliest row.
  const parent = new Map(rows.map((row) => [row.id, row.id]));
  const root = (id) => { while (parent.get(id) !== id) { parent.set(id, parent.get(parent.get(id))); id = parent.get(id); } return id; };
  for (const row of rows) {
    if (row.sameAs == null) continue;
    const [a, b] = [root(row.id), root(row.sameAs)];
    if (a === b) continue;
    if (order.get(a) < order.get(b)) parent.set(b, a); else parent.set(a, b);
  }
  const clusters = new Map();
  for (const row of rows) {
    const id = root(row.id);
    const cluster = clusters.get(id) || { id, option: rows[order.get(id)].option, members: [], passes: [], origins: [] };
    cluster.members.push(row.id);
    if (!cluster.passes.includes(row.pass)) cluster.passes.push(row.pass);
    if (!cluster.origins.includes(row.origin)) cluster.origins.push(row.origin);
    clusters.set(id, cluster);
  }
  const passes = new Map();
  for (const row of rows) {
    const pass = passes.get(row.pass) || { pass: row.pass, options: 0, new: 0, repeats: 0 };
    pass.options += 1;
    if (root(row.id) === row.id) pass.new += 1; else pass.repeats += 1;
    passes.set(row.pass, pass);
  }
  const bag = new Map(rows.map((row) => [row.id, words(`${row.option} ${row.claim || ''}`)]));
  const candidates = [];
  for (let i = 0; i < rows.length; i += 1) {
    for (let j = i + 1; j < rows.length; j += 1) {
      const [a, b] = [rows[i], rows[j]];
      if (root(a.id) === root(b.id)) continue;
      const score = similarity(bag.get(a.id), bag.get(b.id));
      if (score >= minSimilarity) candidates.push({ a: a.id, b: b.id, similarity: Number(score.toFixed(2)), options: [a.option, b.option] });
    }
  }
  candidates.sort((x, y) => y.similarity - x.similarity || order.get(x.a) - order.get(y.a) || order.get(x.b) - order.get(y.b));
  const passList = [...passes.values()];
  const origins = Object.fromEntries(ORIGINS.map((o) => [o, [...clusters.values()].filter((c) => c.origins.includes(o)).length]));
  return {
    options: rows.length,
    distinct: clusters.size,
    passes: passList,
    saturated: passList.length >= 2 && passList.at(-1).new === 0,
    clusters: [...clusters.values()],
    candidates,
    origins,
  };
}

function parseArgs(argv) {
  const opts = { files: [], minSimilarity: DEFAULT_MIN_SIMILARITY };
  for (let i = 0; i < argv.length; i += 1) {
    const flag = argv[i];
    if (flag === '--help' || flag === '-h') { console.log(USAGE); process.exit(0); }
    if (flag !== '--options' && flag !== '--min-similarity') fail('invalidArguments', `Unknown option: ${flag} (see --help)`, 2);
    const value = argv[i + 1];
    if (value == null || value.startsWith('--')) fail('invalidArguments', `missing value for ${flag}`, 2);
    i += 1;
    if (flag === '--options') opts.files.push(value);
    else opts.minSimilarity = Number(value);
  }
  if (!opts.files.length) fail('invalidArguments', 'at least one --options file is required (see --help)', 2);
  if (!Number.isFinite(opts.minSimilarity) || opts.minSimilarity < 0 || opts.minSimilarity > 1) fail('invalidArguments', '--min-similarity must be a number from 0 to 1', 2);
  return opts;
}

if (isMainModule(import.meta.url)) {
  const opts = parseArgs(process.argv.slice(2));
  let result;
  try {
    const rows = readOptionFiles(opts.files);
    if (!rows.length) fail('noOptions', `no option rows in ${opts.files.join(', ')}`, 1);
    result = poolOptions(rows, { minSimilarity: opts.minSimilarity });
  } catch (error) {
    if (error instanceof InvalidOptions) fail('invalidOptions', error.message, 2);
    throw error;
  }
  console.log(JSON.stringify({ ok: true, ...result }, null, 2));
}
