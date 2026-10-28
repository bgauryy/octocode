import fs from 'node:fs';
import path from 'node:path';
import { nextHints } from '../responses.mjs';
import { errorCodes } from './driver.mjs';
import { equal, orderedEqual, scenario } from './advanced-local/helpers.mjs';

const NAME = 'GraphLookupShared';
const files = Array.from({ length: 25 }, (_, index) => `graph-lookup-${String(index).padStart(2, '0')}.ts`);
const ids = files.map(file => `${file}#${NAME}`);
const nearNames = [`${NAME}Extra`, `Prefix${NAME}`, NAME.toLowerCase()];
const nearFile = 'graph-lookup-near.ts';

export function prepareGraphLookup(cwd) {
  for (const file of files) fs.writeFileSync(path.join(cwd, file), `export function ${NAME}() {}\n`);
  fs.writeFileSync(path.join(cwd, nearFile), nearNames.map(name => `export function ${name}() {}`).join('\n'));
}

const queryRows = report => report.responses.filter(entry => entry.tool === 'graphQuery')
  .flatMap(entry => entry.result.json?.results ?? []).map(row => row.data);
const candidateIds = report => queryRows(report).flatMap(row => row.candidates ?? (row.op === 'candidates' ? row.results ?? [] : []))
  .map(row => row.id);

async function candidates(ctx, graph, checks, { target, label }) {
  const result = await scenario(ctx, 'graphQuery', 'cli', {
    label: `graph.lookup.${label}`, checks, responseChars: 700,
    query: { operation: 'candidates', graph, target, limit: 7 },
  });
  return result.data.flatMap(row => row.results ?? []).map(row => row.id);
}

async function ambiguity(ctx, graph, checks, { query, label }) {
  const report = await ctx.explore('graphQuery', { queries: [{ graph, ...query }], responseChars: 700 }, 'cli');
  const actual = candidateIds(report);
  const detail = JSON.stringify({ rows: actual, failures: report.failures, errors: errorCodes(report.root.json) });
  checks.push([`graph.lookup.${label}.typed`, errorCodes(report.root.json).includes('invalidInput')
    && report.failures.length === 0, detail]);
  equal(checks, `graph.lookup.${label}.complete-once`, actual, ids);
  const rows = queryRows(report);
  const continued = rows.filter(row => row.op === 'candidates');
  checks.push([`graph.lookup.${label}.pinned-continuation`, continued.some(row => row.results?.length)
    && rows.every(row => row.graph?.id === graph && row.total === ids.length)
    && continued.every(row => row.offset === 20), JSON.stringify(continued)]);
  orderedEqual(checks, `graph.lookup.${label}.preview`, rows.flatMap(row => row.candidates ?? []).map(row => row.id), ids.slice(0, 20));
  orderedEqual(checks, `graph.lookup.${label}.tail`, continued.flatMap(row => row.results ?? []).map(row => row.id), ids.slice(20));
  const emitted = new Set(report.responses.flatMap(entry => nextHints(entry.result.json))
    .filter(lead => lead.tool === 'graphQuery' && lead.kind === 'continue').map(lead => JSON.stringify(lead.input)));
  const resumed = report.responses.filter(entry => entry.tool === 'graphQuery').slice(1);
  checks.push([`graph.lookup.${label}.opaque-issued-continuations`, resumed.length > 0 && resumed.every(entry =>
    Object.keys(entry.input).length === 1 && typeof entry.input.cursor === 'string' && entry.input.cursor.length > 0
    && emitted.has(JSON.stringify(entry.input))), `${resumed.length} resumed calls`]);
  return actual;
}

async function checkFindWindows(ctx, graph, checks) {
  const rows = [];
  for (const [limit, responseChars] of [[200, 20000], [3, 700]]) {
    const result = await scenario(ctx, 'graphQuery', 'cli', {
      label: `graph.lookup.find.${limit}`, checks, responseChars,
      query: { operation: 'find', graph, target: NAME, kind: 'symbol', limit },
    });
    rows.push(result.data.flatMap(row => row.results ?? []).map(row => row.id));
  }
  const expected = [...ids, ...nearNames.map(name => `${nearFile}#${name}`)];
  equal(checks, 'graph.lookup.find.exact-population', rows[0], expected);
  orderedEqual(checks, 'graph.lookup.find.window-order', rows[0], rows[1]);
}

export async function checkGraphLookup(ctx, graph, checks) {
  const exact = await candidates(ctx, graph, checks, { target: NAME, label: 'exact' });
  const normalized = await candidates(ctx, graph, checks, { target: `  ./${NAME}  `, label: 'normalized' });
  equal(checks, 'graph.lookup.candidates.exact-population', exact, ids);
  orderedEqual(checks, 'graph.lookup.candidates.normalized-order', exact, normalized);
  const single = await candidates(ctx, graph, checks, { target: ids[0], label: 'exact-id' });
  const missing = await candidates(ctx, graph, checks, { target: NAME.toUpperCase(), label: 'case-sensitive-miss' });
  equal(checks, 'graph.lookup.candidates.exact-id', single, [ids[0]]);
  equal(checks, 'graph.lookup.candidates.case-sensitive', missing, []);
  const ambiguous = await ambiguity(ctx, graph, checks, { query: { operation: 'node', target: `./${NAME}` }, label: 'ambiguous-node' });
  orderedEqual(checks, 'graph.lookup.ambiguity.exact-order', exact, ambiguous);
  await ambiguity(ctx, graph, checks, { query: { operation: 'path', target: ids[0], to: `./${NAME}` }, label: 'ambiguous-destination' });
  await checkFindWindows(ctx, graph, checks);
}
