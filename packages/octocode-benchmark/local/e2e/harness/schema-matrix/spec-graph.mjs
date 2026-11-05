// graphBuild and graphQuery. An operation's rows are read by hand from the fixture;
// a filter's effect needs no table: its rows must be exactly the unfiltered rows that
// carry the requested value, so a filter that drops or leaks a row fails.
import { firstDifference } from './runner.mjs';

const ROOT = 'matrix-world/graph';
const ALL_EDGES = ['contains', 'imports', 'uses', 'calls', 'inherits', 'passes'];
const ANCHORS = {
  stats: {},
  find: { target: 'a' },
  candidates: { target: 'a' },
  node: { target: 'src/a.ts' },
  symbols: { target: 'src/a.ts' },
  deps: { target: 'src/a.ts' },
  dependents: { target: 'src/b.ts' },
  callers: { target: 'src/b.ts#b' },
  callees: { target: 'src/a.ts#a' },
  walk: { target: 'src/a.ts' },
  path: { target: 'src/a.ts', to: 'src/c.ts' },
  cycles: {},
  hubs: {},
  diagnostics: {},
  witnesses: { target: 'src/a.ts#a' },
  unresolved: {},
  stale: {},
  issues: {},
  impact: { target: 'src/b.ts' },
  drift: { baseline: 'matrix-baseline' },
};
/** What each operation must name, hand-read from the fixture. */
const MUST_NAME = {
  find: ['src/a.ts#a'],
  candidates: ['src/a.ts#a'],
  symbols: ['src/a.ts#a', 'src/a.ts#Base', 'src/a.ts#Child'],
  deps: ['src/b.ts', 'pkg:declared-dep', 'pkg:undeclared-pkg'],
  dependents: ['src/a.ts'],
  callers: ['src/a.ts#Child.run', 'src/a.ts#a'],
  callees: ['src/b.ts#b'],
  walk: ['src/b.ts'],
  hubs: ['src/a.ts', 'src/b.ts', 'src/c.ts'],
  impact: ['src/b.ts', 'src/a.ts', 'src/c.ts', 'test/a.test.ts'],
};

const dataOf = pages => pages.flatMap(page => page.json?.results ?? []).map(row => row.data ?? {});
const rowsOf = pages => dataOf(pages).flatMap(row => row.rows ?? []);
const idOf = row => row.id ?? row.file ?? row.detector ?? JSON.stringify(row);

/** `kind` narrows what a target may name, so the target follows it. */
function withKind(anchor, kind) {
  if (!anchor || !kind || anchor.target === 'a' || !anchor.target) return anchor;
  if (anchor.to && kind !== 'file') return null;
  const stem = anchor.target.split('#')[0];
  return { ...anchor, target: { file: stem, symbol: `${stem}#${stem === 'src/a.ts' ? 'a' : 'b'}`, package: 'pkg:declared-dep' }[kind] };
}

function base(row, assignment) {
  const operation = row.operation;
  const anchor = withKind(ANCHORS[operation], row.kind);
  if (!anchor) return null;
  const traversal = TRAVERSALS.has(operation) && row.view === undefined;
  return { path: ROOT, ...anchor, ...(traversal && { depth: 1 }), ...(assignment.page === 'tiny' && Object.hasOwn(LIMITED, operation) && { limit: 1 }) };
}
const LIMITED = Object.fromEntries(Object.keys(ANCHORS).filter(name => !['stats', 'node', 'path'].includes(name)).map(name => [name, true]));

const FILTERS = {
  kind: (row, value) => row.kind === value,
  detectors: (row, value) => row.detector === value[0],
  reasons: (row, value) => row.reason === value[0],
  minTier: (row, value) => row.tier >= value,
};
const TRAVERSALS = new Set(['deps', 'dependents', 'callers', 'callees', 'walk']);
const CONFIDENCE = { high: 2, medium: 1, low: 0 };
const FILTERS_ROWS = new Set(['find', 'candidates', 'hubs', 'deps', 'dependents', 'callers', 'callees', 'walk']);
const filterOf = query => Object.keys(FILTERS).filter(name => query[name] !== undefined && (name !== 'kind' || FILTERS_ROWS.has(query.operation)));
const groupOf = row => row.file ?? row.id;

/** The same call with its filters removed: the population a filter must select from. */
function unfilteredInput(query) {
  const { limit, confidence, ...rest } = query;
  for (const name of filterOf(query)) delete rest[name];
  if (query.edges && TRAVERSALS.has(query.operation)) rest.edges = ALL_EDGES;
  if (query.operation === 'issues' && query.minTier !== undefined) rest.minTier = 60;
  delete rest.view;
  if (confidence) rest.confidence = 'low';
  return { queries: [rest] };
}

async function walkAll(worker, surface, input) {
  const rows = [];
  let request = input;
  while (request) {
    const result = await worker.call(surface, 'graphQuery', request);
    rows.push(...(result.json?.results ?? []).flatMap(row => row.data?.rows ?? []));
    const next = result.json?.responsePagination?.next ?? result.json?.results?.[0]?.data?.next?.nextPage;
    request = next?.input;
  }
  return rows;
}

/** A row split across response parts repeats its id: one row, whatever its parts. */
const canonicalIds = rows => [...new Set(rows.map(idOf))].map(id => [id]);

const fileCounts = rows => {
  const counts = new Map();
  for (const row of rows) counts.set(groupOf(row), (counts.get(groupOf(row)) ?? 0) + 1);
  return [...counts];
};

/** With an edge filter a traversal lists a subset: one row per node, whichever edge reached it. */
function edgeProblem(query, listed, whole, names) {
  if (query.view === 'files') return null;
  const population = new Set(whole.map(idOf));
  const wrong = listed.find(row => (row.edge !== undefined && row.edge !== query.edges[0]) || !population.has(idOf(row)));
  if (wrong) return `edges ${query.edges[0]}: row ${JSON.stringify(wrong).slice(0, 160)}`;
  const leaked = names.includes('kind') && listed.find(row => row.kind !== query.kind);
  return leaked ? `kind ${query.kind}: row ${JSON.stringify(leaked).slice(0, 160)}` : null;
}

/** Rows that state no confidence can only be held to the widest population. */
function confidenceProblem(query, listed, whole) {
  const population = new Set(whole.map(idOf));
  const leaked = listed.find(row => !population.has(idOf(row)));
  return leaked ? `confidence ${query.confidence}: row outside the low-confidence population: ${idOf(leaked)}` : null;
}

function exactProblem(query, listed, whole, names) {
  const expected = whole.filter(row => names.every(name => FILTERS[name](row, query[name]))
    && (!query.confidence || !Object.hasOwn(row, 'confidence') || CONFIDENCE[row.confidence] >= CONFIDENCE[query.confidence]));
  const difference = query.view === 'files' ? firstDifference(fileCounts(expected), listed.map(row => [row.file, row.edges])) : firstDifference(canonicalIds(expected), canonicalIds(listed));
  return difference ? `${names.join(',') || 'confidence'}: rows differ from the unfiltered rows that carry the value: ${JSON.stringify(difference)}` : null;
}

async function filterProblem(worker, surface, query, listed) {
  const names = filterOf(query);
  const traversalEdges = !!query.edges && TRAVERSALS.has(query.operation);
  if (!names.length && !query.confidence && !traversalEdges) return null;
  const whole = await walkAll(worker, surface, unfilteredInput(query));
  if (traversalEdges) return edgeProblem(query, listed, whole, names);
  if (query.confidence && !listed.some(row => Object.hasOwn(row, 'confidence'))) return confidenceProblem(query, listed, whole);
  return exactProblem(query, listed, whole, names);
}

async function directionProblem(worker, surface, query, listed) {
  if (query.operation === 'hubs' || query.direction !== 'both') return null;
  const one = async direction => walkAll(worker, surface, { queries: [{ ...query, direction, limit: undefined }] });
  const union = new Set([...await one('in'), ...await one('out')].map(idOf));
  const missing = [...union].filter(id => !listed.some(row => idOf(row) === id));
  return missing.length ? `direction both misses ${missing.slice(0, 3).join(', ')}` : null;
}

function nameProblem(query, listed) {
  const owed = MUST_NAME[query.operation];
  if (!owed || filterOf(query).length || query.kind || query.edges || query.confidence || query.direction || query.view) return null;
  const names = new Set(listed.map(idOf));
  const missing = owed.filter(id => !names.has(id));
  return missing.length ? `${query.operation} misses ${missing.join(', ')}: ${[...names].join(', ').slice(0, 200)}` : null;
}

function filesViewProblem(query, listed) {
  if (query.view !== 'files') return null;
  return listed.every(row => row.edges > 0 && typeof row.file === 'string') ? null : `view files: ${JSON.stringify(listed).slice(0, 200)}`;
}

const CYCLE_MEMBERS = { imports: ['src/a.ts', 'src/b.ts', 'src/c.ts'], calls: ['src/a.ts#a', 'src/b.ts#b', 'src/c.ts#c'] };

const OPERATION_CHECKS = {
  cycles: (query, data) => {
    const nodes = data.flatMap(row => row.rows ?? []).flatMap(row => row.nodes ?? []);
    const owed = CYCLE_MEMBERS[query.edges?.[0] ?? 'imports'] ?? [];
    return owed.every(file => nodes.includes(file)) && (owed.length || !nodes.length) ? null : `cycle members ${nodes.join(', ')}`;
  },
  path: (query, data) => {
    if (query.edges && !query.edges.includes('imports')) return null;
    return data[0]?.found && data[0]?.path?.at(-1)?.id === 'src/c.ts' ? null : `path ${JSON.stringify(data[0]?.path)?.slice(0, 200)}`;
  },
  node: (query, data) => (data[0]?.node?.id === query.target ? null : 'node missing'),
  stats: (query, data) => (data[0]?.counts?.nodesByKind?.file > 0 ? null : 'stats has no files'),
  stale: (query, data) => (data[0]?.fresh === true ? null : 'an untouched tree must read fresh'),
};
const operationProblem = (query, data) => OPERATION_CHECKS[query.operation]?.(query, data) ?? null;

function provenanceProblem(query, listed, data) {
  if (query.includeProvenance !== true || !listed.some(row => Number.isInteger(row.line) && row.id)) return null;
  const text = JSON.stringify(data);
  return /sourceEvidence|sourceRange|occurrenceId|sourceDigest|parseAssessment|derivation/.test(text) ? null : 'includeProvenance added nothing';
}

async function judge(worker, { testCase, surface, pages }) {
  const { query } = testCase;
  const listed = rowsOf(pages);
  const data = dataOf(pages);
  return operationProblem(query, data) ?? nameProblem(query, listed) ?? filesViewProblem(query, listed)
    ?? provenanceProblem(query, listed, data) ?? await filterProblem(worker, surface, query, listed) ?? await directionProblem(worker, surface, query, listed);
}

export const graphQuery = {
  tool: 'graphQuery',
  pseudo: [{ key: 'page', values: ['whole', 'tiny'] }],
  base,
  normalize: testCase => { if (testCase.assignment.page === 'tiny') testCase.responseChars = 400; },
  resolve: (ctx, input) => JSON.parse(JSON.stringify(input).replaceAll('matrix-baseline', ctx.graphId)),
  judge,
};

const RECEIPT = { files: 9, symbolsAtLeast: 12 };
export const graphBuild = {
  tool: 'graphBuild',
  base: () => ({ path: ROOT }),
  judge: (worker, { pages }) => {
    const data = dataOf(pages)[0] ?? {};
    return data.counts?.nodesByKind?.file === RECEIPT.files && data.counts.nodesByKind.symbol >= RECEIPT.symbolsAtLeast ? null : `receipt counts ${JSON.stringify(data.counts)}`;
  },
  acceptedOnly: {},
};
