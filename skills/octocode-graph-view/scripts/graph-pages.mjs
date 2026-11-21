import { execFileSync } from 'node:child_process';
import { posix } from 'node:path';
import { cliCommand, octocode } from './lib.mjs';
import { readResponse } from './tools.mjs';

// Bulk reads let the row `limit` end a page, not the response budget: the budget has no maximum (the default
// 20,000 suits an agent's context; this skill writes pages to disk), and 1,000 lean rows stay well under it.
export const RESPONSE_CHARS = 1_000_000;

// The row data of a graphBuild or graphQuery response; a tool error rejects with its code and message, so a caller
// can match `scopeTooBroad` or a tree that changed during the build.
export function dataOf(tool, response) {
  const data = response?.results?.[0]?.data;
  if (!data) throw new Error(`${tool}: no result row in ${JSON.stringify(response).slice(0, 300)}`);
  if (data.errorCode || data.error) throw new Error(`${tool} ${data.errorCode ?? 'error'}: ${data.error ?? JSON.stringify(data)}`);
  return data;
}

// One graph tool call through the CLI's generated tool command (`octocode graphQuery --json '<envelope>'`).
// A fresh query carries the response budget; a continuation is the tool's own input, sent unchanged.
export const graphCall = (tool, input, { onStderr } = {}) => octocode([tool, '--json', JSON.stringify(input)], { partial: true, onStderr });
export const envelope = (row) => ({ queries: [row], responseChars: RESPONSE_CHARS });

// How many sources one `witnesses` or `unresolved` query takes: the contract's list maximum, read once per
// operation from the CLI's schema.
const listMaximum = new Map();
export function sourcesPerQuery(operation) {
  if (listMaximum.has(operation)) return listMaximum.get(operation);
  const [cmd, ...pre] = cliCommand();
  const out = execFileSync(cmd, [...pre, 'schema', 'graphQuery', '--view', 'query', '--select', `operation=${operation}`], { encoding: 'utf8', maxBuffer: Infinity });
  const target = JSON.parse(out).querySchema?.properties?.target;
  const max = (target?.anyOf ?? [target]).find((s) => s?.type === 'array')?.maxItems;
  if (!Number.isInteger(max)) throw new Error(`graphQuery ${operation} schema declares no target list maximum`);
  listMaximum.set(operation, max);
  return max;
}

// The rows a graphQuery row holds in all: `pagination.total` (stated only when exact), which a complete single
// page omits (its rows are then the whole answer). `more`: the reply carries a `next`, so its rows are not all.
export function totalOf(data, more = false) {
  const total = data.pagination?.total ?? (more ? undefined : data.rows?.length);
  return Number.isInteger(total) ? total : undefined;
}

// The exact total of a graphQuery row from its first reply alone (no walk).
export async function graphTotal(row, { run = graphCall } = {}) {
  const reply = await run('graphQuery', envelope({ ...row, limit: 1 }));
  const total = totalOf(dataOf('graphQuery', reply), reply?.next !== undefined);
  if (total === undefined) throw new Error(`graphQuery ${row.operation} returned no total`);
  return total;
}

// A build's stderr progress notes (`# graphBuild: parsed 120/400 files`) reach `onProgress` without the note mark and tool prefix.
export async function graphBuild(row, { run = graphCall, onProgress = () => {} } = {}) {
  const onStderr = (line) => onProgress(line.replace(/^# graphBuild: /, ''));
  let input = envelope(row);
  for (;;) {
    const response = await readResponse('graphBuild', input, { run: (tool, args) => run(tool, args, { onStderr }) });
    const data = dataOf('graphBuild', {results:response.results.slice(-1)});
    const next = data.next?.expandScan;
    if (next?.kind === 'continue' && next.input) { input = next.input; continue; }
    if (!data.id) throw new Error('graphBuild returned no published snapshot or resume continuation');
    return data;
  }
}

// A graph answer names every file and directory from the workspace root, as every tool does: each `path`, each
// reference field that holds a file or directory, and each `path:line` mention. This skill keys files by their
// snapshot key, relative to the snapshot's root (`graph.root`). Each `path` and mention is restated in that key
// space, in place (a config file above the root as `../package.json`), and so is every reference under the root;
// ids that are not paths (a symbol's `file#name`, `pkg:` keys, finding ids) never lie under it and stay. Calls
// (`next`, `related`) keep theirs.
const REFERENCE_FIELDS = new Set(['id', 'subject', 'from', 'to', 'target', 'members', 'dirs', 'nodes', 'rows', 'files', 'reachedFrom', 'affectedEntrypoints', 'testsToRun']);
const MENTION_FIELDS = new Set(['mentionedIn', 'textMentions']);
export function scopePaths(data) {
  const root = data?.graph?.root;
  if (!root || root === '.') return data;
  const underRoot = (text) => text === root || text.startsWith(`${root}/`);
  const reference = (item) => {
    if (typeof item === 'string') return underRoot(item) ? posix.relative(root, item) || '.' : item;
    if (Array.isArray(item)) return item.map(reference);
    visit(item);
    return item;
  };
  const mention = (item) => {
    if (typeof item === 'string') {
      const [, path, line = ''] = /^(.*?)((?::\d+)?)$/.exec(item);
      return posix.relative(root, path) + line;
    }
    if (Array.isArray(item)) return item.map(mention);
    if (item && typeof item === 'object') return Object.fromEntries(Object.entries(item).map(([k, v]) => [k, mention(v)]));
    return item;
  };
  const visit = (value) => {
    if (Array.isArray(value)) return value.forEach(visit);
    if (!value || typeof value !== 'object') return;
    for (const [key, item] of Object.entries(value)) {
      if (key === 'path' && typeof item === 'string') value[key] = posix.relative(root, item);
      else if (key === 'next' || key === 'related' || key === 'graph') continue;
      else if (REFERENCE_FIELDS.has(key)) value[key] = reference(item);
      else if (MENTION_FIELDS.has(key)) value[key] = mention(item);
      else visit(item);
    }
  };
  visit(data);
  return data;
}

// The array a `next.<name>` continuation extends: a cycle row's members, or a summary/baseline evidence list.
function listOf(name, first, rows, pageStart) {
  const cycle = /^cycle(\d+)$/.exec(name);
  if (cycle) {
    const member = rows[pageStart + Number(cycle[1])];
    if (!Array.isArray(member?.nodes)) throw new Error(`Cycle member continuation ${name} has no cycle row`);
    delete member.nodesTruncated;
    return member.nodes;
  }
  const [section, field, extra] = name.split('.');
  const list = first[section]?.[field];
  if (!['summary', 'baseline'].includes(section) || extra || !Array.isArray(list)) throw new Error(`Unknown graph evidence list: ${name}`);
  return list;
}

// Every row of one graphQuery row, read to its end: the reply's `next` pages (one read follows them all, each
// page a slice), every `summary.*`/`baseline.*` evidence
// list (`next['summary.x']`) and every cycle's member list (`next.cycle<i>`, i = the row on its page). Raw pages
// reach save() before they are joined, with each page's data, its paths restated as snapshot keys ([`scopePaths`]),
// which the result keeps too. Pages carry no row offset (it lives in the cursor), so exactly-once is
// checked by count: a list's rows, the ones already shown included, must end equal to its `total`. A repeated
// input, a count mismatch or a missing continuation rejects: the result is complete or there is none. The result
// is the first page's data with `rows` holding every row and no `pagination` or `next`.
export async function readGraph(row, { run = graphCall, save = () => {}, onPage = () => {}, collectionPaths, collectionRefs } = {}) {
  const seen = new Set();
  async function read(input) {
    const key = JSON.stringify(input);
    if (seen.has(key)) throw new Error('Repeated graph continuation; evidence is incomplete');
    seen.add(key);
    const response = await readResponse('graphQuery', input, { run, collectionPaths, collectionRefs, onPage: (raw, call) => save(call.input, raw, scopePaths(structuredClone(dataOf('graphQuery', raw)))) });
    return response.results.map((result) => {
      const data = scopePaths(structuredClone(dataOf('graphQuery', {results:[result]})));
      onPage(data);
      return data;
    });
  }
  async function extras(data, first, rows, pageStart) {
    for (const [name, call] of Object.entries(data.next ?? {})) {
      if (call?.kind !== 'continue') continue;
      const list = listOf(name, first, rows, pageStart);
      const tail = await collect(call.input, list.length);
      list.push(...tail.rows);
    }
  }
  // `shown`: rows of this list an earlier page already carried (a summary list or a cycle's first members).
  async function collect(input, shown = 0) {
    // One read follows the reply's `next` to its end: every page of the row is a slice.
    const slices = await read(input);
    const first = slices[0];
    const total = slices.map((data) => data.pagination?.total).find(Number.isInteger);
    const rows = [];
    // A page without `rows` is a contract change, not an empty answer: fail rather than build an empty graph.
    for (const data of slices) {
      if (!Array.isArray(data.rows)) throw new Error(`graphQuery page has no rows array: ${Object.keys(data).join(',')}`);
      const pageStart = rows.length;
      rows.push(...data.rows);
      if (Number.isInteger(total) && shown + rows.length > total) throw new Error('Graph pages repeat rows: more rows than pagination.total');
      await extras(data, first, rows, pageStart);
    }
    if (Number.isInteger(total) && shown + rows.length !== total) throw new Error('Graph evidence ends without a required continuation');
    first.rows = rows;
    delete first.next;
    delete first.pagination;
    return first;
  }
  return collect(envelope(row));
}
