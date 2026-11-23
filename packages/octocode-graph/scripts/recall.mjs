#!/usr/bin/env node
// Scores a saved graph's callers against hand-checked call sites and reports its build phases, so a change to
// stitching, LSP probing or the engine is measured, not guessed.
//
//   node scripts/recall.mjs <result-dir> [--truth <dir>] [--sample N] [--seed S]
//
// <dir> holds functions.csv (target,function,declared_at,category) and ground_truth_sites.csv
// (target,function,path,line,kind,caller,test,<target>_category); default: the 20.2.0 graph-vs-tools round.
// Recall asks `callers <file#function> --tests` per function, as an agent does, and counts the ground-truth call
// sites its rows cite. --sample N checks N stitched cross-chunk calls (seeded) against lspSearch `definition` at the
// call site: agreement is the precision guardrail for bindings no ground truth covers. Run it from the
// repository root the graph was built in (lspSearch reads paths under the workspace). Prints one JSON object.
import { readFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { parseArgs } from 'node:util';
import { openGraph } from '../src/runtime/api.ts';
import { octocode } from '../src/runtime/lib.ts';

const here = new URL('.', import.meta.url).pathname;
const ROUND = resolve(
  here,
  '../../octocode-benchmark/results/rounds/20.2.0-2026-11-23-graph-vs-tools/data'
);

const { values, positionals } = parseArgs({
  options: {
    truth: { type: 'string', default: ROUND },
    sample: { type: 'string', default: '0' },
    seed: { type: 'string', default: '7' },
  },
  allowPositionals: true,
});
if (positionals.length !== 1) {
  process.stderr.write(
    'usage: node scripts/recall.mjs <result-dir> [--truth <dir>] [--sample N] [--seed S]\n'
  );
  process.exit(2);
}

const csv = file => {
  const [head, ...rows] = readFileSync(join(values.truth, file), 'utf8')
    .replace(/\r/g, '')
    .trim()
    .split('\n');
  const keys = head.split(',');
  return rows.map(r =>
    Object.fromEntries(r.split(',').map((v, i) => [keys[i], v]))
  );
};

const graph = openGraph(positionals[0]);
const D = graph.data;
const root = D.section;
// The ground truth's paths are relative to the graph root the round measured.
const functions = csv('functions.csv');
const sites = csv('ground_truth_sites.csv').filter(s => s.kind === 'call');
const categoryOf = s => s[`${s.target}_category`] ?? s.category ?? '-';

const recall = { found: 0, total: 0, extra: 0, byCategory: {}, misses: [] };
for (const f of functions) {
  const file = f.declared_at.split(':')[0];
  // Only functions of this graph's target: a TypeScript row is not in a Rust graph.
  if (!D.files.some(x => x.id === file)) continue;
  const r = graph.query(`callers ${file}#${f.function} --tests`);
  const cited = new Set();
  for (const row of r.rows ?? [])
    for (const line of row.lines ?? [row.loc?.line])
      cited.add(`${row.loc.path}:${line}`);
  const truth = sites.filter(
    s => s.function === f.function && s.target === f.target
  );
  const expected = new Set(truth.map(s => `${s.path}:${s.line}`));
  for (const s of truth) {
    const hit = cited.has(`${s.path}:${s.line}`);
    const c = (recall.byCategory[categoryOf(s)] ??= { found: 0, total: 0 });
    c.total++;
    recall.total++;
    if (hit) {
      c.found++;
      recall.found++;
    } else
      recall.misses.push({
        function: `${file}#${f.function}`,
        site: `${s.path}:${s.line}`,
        category: categoryOf(s),
      });
  }
  for (const site of cited) if (!expected.has(site)) recall.extra++;
}

// Phases of the run's last build or refresh, from its event log.
const events = readFileSync(
  join(resolve(positionals[0]), 'events.jsonl'),
  'utf8'
)
  .trim()
  .split('\n')
  .map(line => JSON.parse(line));
const lastRun = events.findLastIndex(
  e => e.type === 'run' || e.type === 'refresh'
);
const phases = {};
for (const e of events.slice(Math.max(0, lastRun)))
  if (e.type === 'phase') {
    const p = (phases[e.id] ??= {});
    if (e.state === 'start') p.start = e.t;
    else p.ms = e.t - (p.start ?? e.t);
  }

// Seeded sample of stitched cross-chunk calls, checked against the language server's definition at the call site.
async function agreement(n, seed) {
  const stitched = D.calls.filter(
    c =>
      c.crossChunk &&
      c.via === 'graph' &&
      c.resolution?.includes('stitched-import')
  );
  let state = seed;
  const next = () =>
    (state = (state * 1103515245 + 12345) % 2147483648) / 2147483648;
  const picked = new Map();
  while (picked.size < Math.min(n, stitched.length)) {
    const i = Math.floor(next() * stitched.length);
    picked.set(i, stitched[i]);
  }
  const checked = { sample: picked.size, agree: 0, differ: [], unanswered: 0 };
  const calls = [...picked.values()];
  for (let i = 0; i < calls.length; i += 5) {
    const batch = calls.slice(i, i + 5);
    const input = {
      queries: batch.map(c => ({
        operation: 'definition',
        path: join(root, c.source.split('#')[0]),
        symbolName: c.target
          .split('#')[1]
          .split('.')
          .pop()
          .replace(/@\d+$/, ''),
        line: c.line,
      })),
    };
    for (const [k, c] of batch.entries()) {
      let files = [];
      try {
        const reply = await octocode(
          ['lspSearch', JSON.stringify({ queries: [input.queries[k]] })],
          { partial: true }
        );
        files = (reply?.results?.[0]?.data?.files ?? []).map(f =>
          resolve(root, f.path)
        );
      } catch (error) {
        // The definition lies outside the allowed read roots (an external crate or package): not the bound target.
        if (String(error).includes('locationsWithheld')) {
          checked.differ.push(
            `${c.source}:${c.line} -> ${c.target} (defined outside the workspace)`
          );
          continue;
        }
        checked.unanswered++;
        continue;
      }
      if (!files.length) checked.unanswered++;
      else if (files.includes(join(root, c.target.split('#')[0])))
        checked.agree++;
      else checked.differ.push(`${c.source}:${c.line} -> ${c.target}`);
    }
  }
  return checked;
}

const n = Number(values.sample);
process.stdout.write(
  `${JSON.stringify(
    {
      graph: graph.path,
      recall,
      stitchedCalls: D.calls.filter(c => c.crossChunk && c.via === 'graph')
        .length,
      callStats: {
        lsp: D.callStats?.lsp,
        probed: D.callStats?.probed,
        resolved: D.callStats?.resolved,
        withheld: D.callStats?.withheld,
        unanswered: D.callStats?.unanswered?.length,
      },
      phases,
      ...(n > 0 && { agreement: await agreement(n, Number(values.seed)) }),
    },
    null,
    1
  )}\n`
);
