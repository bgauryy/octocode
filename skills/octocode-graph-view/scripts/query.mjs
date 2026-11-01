#!/usr/bin/env node
// The graph query language over one data.json (pure functions; the server, the channel and this CLI share it).
//   impact <file> [depth N]        every file that transitively depends on <file>
//   deps | dependents <file> [depth N]   forward / reverse neighbors (default depth 1)
//   path <a> <b>                   the strongest dependency path a → b (Dijkstra, cost 1/weight)
//   cycle <file>                   its strongly connected set, ordered along one ring through it
//   community <file|id>            members by PageRank and links to other communities
//   callers | callees <file#symbol|name>   symbol-level calls with provenance
//   top <metric> [n|all]           files ranked by pagerank, betweenness, inWeight, outWeight, core or depth
//   find <text>                    files and symbols whose path or name contains the text
// `--tests` includes test-only edges; `--imports` walks import edges only; `--weak` adds edges backed only by
// medium/low-confidence name-bound calls (walks follow strong edges by default). Result: { ok, op, text, error?, candidates?, nodes, edges, rows, summary, stats }.
import { readFileSync, realpathSync, statSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { QueryError, indexOf, resolveFile } from './query-index.mjs';
import * as ops from './query-ops.mjs';

export const OPS = {
  impact: ops.impact, deps: ops.deps, dependents: ops.dependents, path: ops.path, cycle: ops.cycle,
  community: ops.community, callers: ops.callers, callees: ops.callees, top: ops.top, find: ops.find,
};
export const HINT = 'impact <file> [depth N] · deps|dependents <file> [depth N] · path <a> <b> · cycle <file> · community <file|id> · callers|callees <file#symbol|name> · top <metric> [n|all] · find <text> · --tests · --imports · --weak';

// Words, with "double" or 'single' quotes keeping spaces.
export function tokenize(text) {
  return [...String(text ?? '').matchAll(/"([^"]*)"|'([^']*)'|(\S+)/g)].map((m) => m[1] ?? m[2] ?? m[3]);
}

const DEPTH = /^(?:--)?depth(?:=(.+))?$/i;
function depthValue(v) {
  if (/^(all|\*|inf)$/i.test(String(v))) return Infinity;
  const n = Number.parseInt(v, 10);
  if (!Number.isInteger(n) || n < 1) throw new QueryError(`depth must be a positive integer or all, not ${v ?? 'nothing'}`);
  return n;
}

// Ops whose walk takes a depth; elsewhere `depth` is an ordinary word (top depth, find depth).
const WALKS = new Set(['impact', 'deps', 'dependents']);

// Arguments, `--tests`, `--imports`, `--weak`, and (for walks) `depth N` / `--depth N` / `depth=N` anywhere after the op.
export function parseArgs(tokens, { walk = true } = {}) {
  const args = [];
  let tests = false, imports = false, weak = false, depth = null;
  for (let i = 0; i < tokens.length; i++) {
    const t = tokens[i];
    const m = walk ? DEPTH.exec(t) : null;
    if (t === '--tests') tests = true;
    else if (t === '--imports') imports = true;
    else if (t === '--weak') weak = true;
    else if (m) depth = depthValue(m[1] ?? tokens[++i]);
    else args.push(t);
  }
  return { args, tests, imports, weak, depth };
}

// Each candidate as the same query with the ambiguous argument replaced, so a viewer or agent can re-run it.
function candidateRows(tokens, error) {
  return error.candidates.map((id) => ({ id, query: tokens.map((t) => (t === error.arg ? (/\s/.test(id) ? `"${id}"` : id) : t)).join(' ') }));
}

const empty = { nodes: [], edges: [], rows: [] };

/** Runs one query text over data `D`; never throws for a bad query (ok:false with error and candidates). */
export function runQuery(D, text) {
  const q = String(text ?? '').trim();
  const tokens = tokenize(q);
  const op = (tokens[0] ?? '').toLowerCase();
  const base = { op, text: q };
  if (!Object.hasOwn(OPS, op)) {
    const error = op ? `unknown query "${op}"; try: ${HINT}` : `empty query; try: ${HINT}`;
    return { ok: false, ...base, error, ...empty, summary: error };
  }
  try {
    const p = parseArgs(tokens.slice(1), { walk: WALKS.has(op) });
    const r = OPS[op](indexOf(D, { tests: p.tests, imports: p.imports, weak: p.weak }), p);
    return { ok: true, ...base, ...empty, ...r, ...(p.tests ? { tests: true } : {}), ...(p.imports ? { imports: true } : {}), ...(p.weak ? { weak: true } : {}) };
  } catch (error) {
    if (!(error instanceof QueryError)) throw error;
    return { ok: false, ...base, error: error.message, ...(error.candidates.length ? { candidates: candidateRows(tokens, error) } : {}), ...empty, summary: error.message };
  }
}

/** Node arguments (paths, suffixes, ids, file#symbol) → ids; anything unknown or ambiguous is listed, not guessed. */
export function resolveNodes(D, list) {
  const I = indexOf(D);
  const ids = [];
  const problems = [];
  for (const arg of list ?? []) {
    try {
      ids.push(resolveFile(I, arg));
    } catch (error) {
      if (!(error instanceof QueryError)) throw error;
      problems.push({ arg: String(arg), error: error.message, candidates: error.candidates });
    }
  }
  return { ids: [...new Set(ids)], problems };
}

function rowLine(r) {
  const tags = [
    r.depth != null ? `d${r.depth}` : null, r.rank != null ? `#${r.rank}` : null, r.order != null ? `ring ${r.order}` : null,
    r.weight != null ? `w ${r.weight}` : null, r.value != null ? `${r.value}` : null, r.pagerank != null ? `pr ${r.pagerank}` : null,
    r.symbol ? `${r.name} (${r.via}${r.confidence ? `/${r.confidence}` : ''}${r.count > 1 ? ` ×${r.count}` : ''})` : null,
    r.through ? `via ${r.through}` : null, r.next ? `→ ${r.next}` : null,
  ].filter(Boolean);
  return `${r.loc.path}:${r.loc.line}${tags.length ? `  ${tags.join('  ')}` : ''}`;
}

/** Plain text of a result: the summary, every row with its location, links or candidates. */
export function formatResult(r) {
  const lines = [`${r.ok ? '' : 'error: '}${r.summary ?? r.error ?? ''}`];
  for (const x of r.rows ?? []) lines.push(`  ${rowLine(x)}`);
  for (const l of r.links ?? []) lines.push(`  link community ${l.community}${l.label ? ` (${l.label})` : ''}: weight ${l.weight} (out ${l.out}, in ${l.in})`);
  if (r.candidates?.length) lines.push('candidates (re-run one):', ...r.candidates.map((c) => `  ${c.query}`));
  return lines.join('\n');
}

function dataPath(arg) {
  const p = resolve(arg);
  return statSync(p, { throwIfNoEntry: false })?.isDirectory() ? join(p, 'data.json') : p;
}

function main(argv) {
  const json = argv.includes('--json');
  const rest = argv.filter((a) => a !== '--json');
  if (rest.length < 2 || argv.includes('--help')) {
    process.stderr.write(`usage: query.mjs <result-dir|data.json> "<query>" [--json]\n  ${HINT}\n  Offline; with a running viewer, reply.mjs query <out> "<query>" also highlights the answer.\n`);
    process.exitCode = 2;
    return;
  }
  const D = JSON.parse(readFileSync(dataPath(rest[0]), 'utf8'));
  const r = runQuery(D, rest.slice(1).join(' '));
  process.stdout.write(`${json ? JSON.stringify(r) : formatResult(r)}\n`);
  if (!r.ok) process.exitCode = 1;
}

const isMain = () => {
  try { return Boolean(process.argv[1]) && realpathSync(process.argv[1]) === realpathSync(fileURLToPath(import.meta.url)); } catch { return false; }
};
if (isMain()) main(process.argv.slice(2));
