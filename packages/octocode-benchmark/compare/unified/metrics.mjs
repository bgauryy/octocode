#!/usr/bin/env node
// Read-only metrics over a recorded run (RFC tool-quality-efficiency S1):
//   - page-hint follow rate per worker: of the page continuations offered in
//     tool results (`next` entries named nextPage / next* / continue* / restart /
//     searchUnpatchedFile / binarySkipped, plus responsePagination.next), the
//     share a later call of the same session followed;
//   - weighted tokens and Q/$ under the frozen tariff (lib.mjs TARIFF), warm
//     and cold cache, checked against the recorded cost;
//   - input-validation (schema) errors per worker.
// Writes nothing into the run directory.
//
//   node metrics.mjs --run-id <id> [--json] [--self-test]
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { RESULTS_DIR, TARIFF, parseArgs, readJson, weightedUsage } from './lib.mjs';

// `next` holds page continuations; leads move to `hints`. Today's `next` also
// carries leads, so pages are selected by name in either shape.
const PAGE_NAME = /^(?:next(?:[A-Z]\w*)?|continue\w*|restart|searchUnpatchedFile|binarySkipped)$/;
const BRIEF_KEYS = new Set(['goal', 'mainGoal', 'reasoning', 'debug']);
const PAGING_KEY = /page|offset|cursor|after|resume/i;
const IDENTITY_KEYS = ['path', 'uri', 'owner', 'repo', 'number', 'symbolName', 'sha', 'ref'];
const isHint = v => !!v && typeof v === 'object' && !Array.isArray(v) && typeof v.tool === 'string' && v.query && typeof v.query === 'object';
const canonical = v => (Array.isArray(v) ? `[${v.map(canonical).join(',')}]` : v && typeof v === 'object' ? `{${Object.keys(v).sort().map(k => `${JSON.stringify(k)}:${canonical(v[k])}`).join(',')}}` : JSON.stringify(v));
const withoutBriefs = q => Object.fromEntries(Object.entries(q ?? {}).filter(([k]) => !BRIEF_KEYS.has(k)));

/** Page continuations in one parsed tool result: [{name, tool, query}]. */
export function pageHints(value) {
  const out = [];
  const walk = node => {
    if (!node || typeof node !== 'object') return;
    for (const [key, child] of Object.entries(node)) {
      if (key === 'next' && child && typeof child === 'object') {
        if (isHint(child)) out.push({ name: 'next', ...child });
        else if (!Array.isArray(child)) for (const [name, h] of Object.entries(child)) if (isHint(h) && PAGE_NAME.test(name)) out.push({ name, ...h });
      } else walk(child);
    }
  };
  walk(value);
  return out;
}

/** A row matches a hint query verbatim when every non-brief hint field is equal. */
const subset = (hintQuery, row) => Object.entries(withoutBriefs(hintQuery)).every(([k, v]) => canonical(v) === canonical(row?.[k]));
function verbatim(hint, input) {
  if (Array.isArray(hint.query.queries)) {
    const { queries, ...top } = hint.query;
    return subset(top, input) && Array.isArray(input?.queries) && queries.every((q, i) => subset(q, input.queries[i]));
  }
  return (Array.isArray(input?.queries) ? input.queries : [input]).some(row => subset(hint.query, row));
}
/** Loose follow: same paging values and identity fields (the agent re-typed the call); verbatim when the hint has no paging field. */
function loose(hint, input) {
  if (verbatim(hint, input)) return true;
  const rows = Array.isArray(input?.queries) ? input.queries : [input];
  const query = Array.isArray(hint.query.queries) ? { ...hint.query, ...hint.query.queries[0] } : hint.query;
  const paging = Object.keys(query).filter(k => PAGING_KEY.test(k) && typeof query[k] !== 'object');
  if (!paging.length) return false;
  const merged = Array.isArray(hint.query.queries) ? rows.map(r => ({ ...input, ...r })) : rows;
  return merged.some(row => paging.every(k => canonical(row?.[k]) === canonical(query[k])) && IDENTITY_KEYS.every(k => query[k] === undefined || canonical(row?.[k]) === canonical(query[k])));
}

/** Follow rate of one session's stream: offered page hints, followed (loose) and followed verbatim. */
export function followRate(streamText) {
  const events = String(streamText).split('\n').filter(l => l.trim()).map(l => { try { return JSON.parse(l); } catch { return null; } }).filter(Boolean);
  const calls = [];
  const offered = new Map();
  let schemaErrors = 0;
  for (const e of events) {
    if (e.type === 'assistant' && !e.parent_tool_use_id) {
      for (const c of e.message?.content ?? []) if (c.type === 'tool_use') calls.push({ at: calls.length, tool: String(c.name).split('__').at(-1), input: c.input ?? {} });
    } else if (e.type === 'user') {
      for (const c of e.message?.content ?? []) {
        if (c.type !== 'tool_result') continue;
        const texts = typeof c.content === 'string' ? [c.content] : (c.content ?? []).filter(x => x.type === 'text').map(x => x.text);
        for (const text of texts) {
          if (/^Input validation error|Invalid arguments for tool|MCP error -32602/.test(text)) schemaErrors += 1;
          let parsed; try { parsed = JSON.parse(text); } catch { continue; }
          for (const h of pageHints(parsed)) {
            const key = `${h.tool}:${canonical(withoutBriefs(h.query))}`;
            if (!offered.has(key)) offered.set(key, { ...h, after: calls.length });
          }
        }
      }
    }
  }
  const rows = [...offered.values()].map(h => {
    const later = calls.slice(h.after).filter(c => c.tool === h.tool);
    return { name: h.name, tool: h.tool, followed: later.some(c => loose(h, c.input)), verbatim: later.some(c => verbatim(h, c.input)), lastTurn: h.after >= calls.length };
  });
  return { offered: rows.length, followed: rows.filter(r => r.followed).length, verbatim: rows.filter(r => r.verbatim).length, offeredAtEnd: rows.filter(r => r.lastTurn).length, rows, schemaErrors };
}

function selfTest() {
  const ev = o => JSON.stringify(o);
  const result = (id, body) => ev({ type: 'user', message: { content: [{ type: 'tool_result', tool_use_id: id, content: [{ type: 'text', text: JSON.stringify(body) }] }] } });
  const call = (id, name, input) => ev({ type: 'assistant', message: { id, content: [{ type: 'tool_use', id, name: `mcp__octocode__${name}`, input }] } });
  const page = { tool: 'localSearch', query: { goal: 'g', path: 'src', searchText: 'x', page: 2, snapshot: 's' } };
  const lead = { tool: 'localFetch', query: { path: 'src/a.ts' } };
  const stream = [
    call('a', 'localSearch', { path: 'src', searchText: 'x' }),
    result('a', { results: [{ data: { next: { nextPage: page, readFile: lead } } }] }),
    call('b', 'localSearch', { queries: [{ goal: 'other', path: 'src', searchText: 'x', page: 2, snapshot: 's' }] }),
    result('b', { results: [{ data: { next: { nextPage: { ...page, query: { ...page.query, page: 3 } } } } }], responsePagination: { next: { tool: 'localSearch', query: { queries: [{ path: 'src' }], responseCharOffset: 900 } } } }),
    call('c', 'localSearch', { path: 'src', searchText: 'x', page: 3 }),
    ev({ type: 'user', message: { content: [{ type: 'tool_result', tool_use_id: 'c', is_error: true, content: 'Input validation error: Invalid arguments for tool localSearch' }] } }),
  ].join('\n');
  const f = followRate(stream);
  const assert = (ok, msg) => { if (!ok) { console.error('FAIL', msg, JSON.stringify(f)); process.exit(1); } };
  assert(f.offered === 3, 'three page hints offered (lead excluded, envelope included)');
  assert(f.verbatim === 1 && f.followed === 2, 'page 2 verbatim (brief differs), page 3 re-typed without snapshot is followed loosely, envelope not followed');
  assert(f.schemaErrors === 1, 'schema error counted');
  assert(pageHints({ hints: { readFixPr: lead }, next: { continuePatch: page } }).length === 1, 'hints leads are not page hints');
  console.log('metrics self-test: ok');
}

const pct = (a, b) => (b ? `${((100 * a) / b).toFixed(1)}%` : '—');
function main() {
  const args = parseArgs(process.argv.slice(2));
  if (args['self-test']) return selfTest();
  if (!args['run-id']) throw new Error('--run-id is required');
  const runDir = path.join(RESULTS_DIR, String(args['run-id']));
  const manifest = readJson(path.join(runDir, 'manifest.json'));
  const summaryFile = path.join(runDir, 'summary.json');
  const summary = fs.existsSync(summaryFile) ? readJson(summaryFile) : null;
  const quality = (q, w) => summary?.rows?.find(r => r.qid === q)?.w?.[w]?.quality ?? null;
  const out = { runId: manifest.runId, tariff: TARIFF, workers: {} };
  for (const w of manifest.workers) {
    const acc = { questions: 0, quality: 0, cost: 0, tariffCost: 0, tariffCostCold: 0, weighted: 0, weightedCold: 0, total: 0, weightedKnown: true, offered: 0, followed: 0, verbatim: 0, offeredAtEnd: 0, schemaErrors: 0, byName: {} };
    for (const q of manifest.questionIds) {
      const dir = path.join(runDir, 'runs', q, w);
      if (!fs.existsSync(path.join(dir, 'run.json'))) continue;
      const r = readJson(path.join(dir, 'run.json'));
      const t = r.tokens;
      const wt = t.weighted_tokens != null ? t : { ...t, ...weightedUsage(t, { models: r.actualModels ?? [], cacheCreation: t.cache_creation ?? null, firstRequestCacheRead: r.perRequest?.[0]?.cache_read_input_tokens ?? 0 }) };
      acc.questions += 1; acc.quality += quality(q, w) ?? 0; acc.cost += r.cost_usd ?? 0; acc.total += t.total_tokens;
      if (wt.weighted_tokens == null) acc.weightedKnown = false;
      else { acc.weighted += wt.weighted_tokens; acc.weightedCold += wt.weighted_tokens_cold; acc.tariffCost += wt.tariff_cost_usd; acc.tariffCostCold += wt.tariff_cost_usd_cold; }
      const stream = path.join(dir, 'stream.jsonl');
      if (!fs.existsSync(stream)) continue;
      const f = followRate(fs.readFileSync(stream, 'utf8'));
      acc.offered += f.offered; acc.followed += f.followed; acc.verbatim += f.verbatim; acc.offeredAtEnd += f.offeredAtEnd; acc.schemaErrors += f.schemaErrors;
      for (const row of f.rows) { const b = (acc.byName[row.name] ??= { offered: 0, followed: 0 }); b.offered += 1; b.followed += row.followed ? 1 : 0; }
    }
    out.workers[w] = {
      ...acc,
      followRate: acc.offered ? acc.followed / acc.offered : null, verbatimRate: acc.offered ? acc.verbatim / acc.offered : null,
      qPer10k: acc.quality / (acc.total / 10_000), qPer10kWeighted: acc.weightedKnown ? acc.quality / (acc.weighted / 10_000) : null, qPer10kWeightedCold: acc.weightedKnown ? acc.quality / (acc.weightedCold / 10_000) : null,
      qPerDollar: acc.cost ? acc.quality / acc.cost : null, qPerDollarCold: acc.weightedKnown ? acc.quality / acc.tariffCostCold : null,
      tariffReproducesCost: acc.weightedKnown ? Math.abs(acc.tariffCost - acc.cost) < 1e-6 : null,
    };
  }
  if (args.json) { console.log(JSON.stringify(out, null, 2)); return; }
  const W = out.workers;
  console.log(`run ${out.runId} · tariff ${TARIFF.id} (USD/M: input ${TARIFF.usdPerMTok.input}, 1h cache write ${TARIFF.usdPerMTok.cacheWrite1h}, cache read ${TARIFF.usdPerMTok.cacheRead}, output ${TARIFF.usdPerMTok.output})\n`);
  console.log('| worker | Σ quality | total tokens | weighted warm | weighted cold | recorded cost | tariff cost (warm / cold) | Q/10k raw | Q/10k weighted (warm / cold) | Q/$ (warm / cold) | schema errors |');
  console.log('|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|');
  for (const [w, x] of Object.entries(W)) console.log(`| ${w} | ${x.quality.toFixed(2)} | ${x.total} | ${Math.round(x.weighted)} | ${Math.round(x.weightedCold)} | $${x.cost.toFixed(4)} | $${x.tariffCost.toFixed(4)} / $${x.tariffCostCold.toFixed(4)}${x.tariffReproducesCost ? ' ✓' : x.tariffReproducesCost === false ? ' ✗' : ''} | ${x.qPer10k.toFixed(3)} | ${x.qPer10kWeighted?.toFixed(3) ?? '—'} / ${x.qPer10kWeightedCold?.toFixed(3) ?? '—'} | ${x.qPerDollar?.toFixed(1) ?? '—'} / ${x.qPerDollarCold?.toFixed(1) ?? '—'} | ${x.schemaErrors} |`);
  console.log('\n| worker | page hints offered | followed | verbatim | follow rate | verbatim rate | offered in the last result | by name (followed/offered) |');
  console.log('|---|--:|--:|--:|--:|--:|--:|---|');
  for (const [w, x] of Object.entries(W)) console.log(`| ${w} | ${x.offered} | ${x.followed} | ${x.verbatim} | ${pct(x.followed, x.offered)} | ${pct(x.verbatim, x.offered)} | ${x.offeredAtEnd} | ${Object.entries(x.byName).map(([n, b]) => `${n} ${b.followed}/${b.offered}`).join(', ') || '—'} |`);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) main();
