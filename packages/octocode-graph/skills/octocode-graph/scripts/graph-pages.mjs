import { posix, relative, resolve, sep } from 'node:path';
import { octocode } from './lib.mjs';
import { readResponse } from './tools.mjs';

// Bulk walks save evidence to disk. The page budget bounds each reply and every continuation is consumed.
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

// The rows a graphQuery row holds in all: `pagination.total` (stated only when exact), which a complete single
// page omits (its rows are then the whole answer). `more`: the reply carries a `next`, so its rows are not all.
export function totalOf(data, more = false) {
  const total = data.pagination?.total ?? (more ? undefined : data.rows?.length);
  return Number.isInteger(total) ? total : undefined;
}

// A build's stderr progress notes (`# graphBuild: parsed 120/400 files`) reach `onProgress` without the note mark and tool prefix.
export async function graphBuild(row, { run = graphCall, onProgress = () => {} } = {}) {
  const onStderr = (line) => onProgress(line.replace(/^# graphBuild: /, ''));
  let input = envelope(row);
  for (;;) {
    const response = await readResponse('graphBuild', input, { run: (tool, args) => run(tool, args, { onStderr }) });
    const data = dataOf('graphBuild', {results:response.results.slice(-1)});
    const next = data.next?.expandScan;
    if (next?.tool === 'graphBuild' && next.input) { input = next.input; continue; }
    if (!data.graph) throw new Error('graphBuild returned no published snapshot or resume continuation');
    return data;
  }
}

// A graph answer names every file and directory from the workspace root, as every tool does: each `path`, each
// reference field that holds a file or directory, and each `path:line` mention. This skill keys files by their
// snapshot key, relative to the snapshot's root (`graph.root`). Each `path` and mention is restated in that key
// space, in place (a config file above the root as `../package.json`), and so is every reference under the root;
// file-qualified symbol ids keep their suffix. Package keys and finding ids stay intact.
// Executable continuations and related calls keep their original arguments.
const REFERENCE_FIELDS = new Set(['id', 'subject', 'from', 'to', 'target', 'members', 'dirs', 'nodes', 'rows', 'files', 'reachedFrom', 'affectedEntrypoints', 'testsToRun']);
const MENTION_FIELDS = new Set(['mentionedIn', 'textMentions']);
export function scopePaths(data, queryRoot) {
  const scopeRoot = data?.graph?.root ?? queryRoot;
  const root = scopeRoot && relative(process.cwd(), resolve(scopeRoot)).split(sep).join('/');
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
  // An impact page gives the count; the branch carries the actual entries.
  if (Number.isInteger(first.summary?.[name])) {
    first.summary[name] = [];
    return first.summary[name];
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
export async function readGraph(row, options) {
  const [answer] = await readGraphs([row], options);
  return answer;
}

export async function readGraphs(queries, { run = graphCall, save, onPage = () => {}, snapshotRoot, collectionPaths, collectionRefs } = {}) {
  const seen = new Set();
  const snapshots = new Map();
  const scoped = (data, index) => scopePaths(data, snapshotRoot ?? queries[index].path);
  async function read(input, branch) {
    const key = JSON.stringify(input);
    if (seen.has(key)) throw new Error('Repeated graph continuation; evidence is incomplete');
    seen.add(key);
    const indexOf = result => {
      const index = branch ?? result.index ?? (queries.length === 1 ? 0 : undefined);
      if (!queries[index]) throw new Error(`Unexpected graph query result index ${index}`);
      return index;
    };
    const response = await readResponse('graphQuery', input, {
      run, collectionPaths, collectionRefs,
      onPage: save && ((raw, call) => {
        const first = raw.results?.[0];
        save(call.input, raw, first?.data && scoped(structuredClone(first.data), indexOf(first)));
      }),
      onResult: result => {
        const index = indexOf(result);
        // The assembled row is private to this walk. Stream it only after its final fragment.
        const data = scoped(dataOf('graphQuery', { results: [result] }), index);
        const snapshot = data.graph?.id;
        if (snapshot !== undefined) {
          if (snapshots.has(index) && snapshots.get(index) !== snapshot) throw new Error('Graph snapshot changed during pagination');
          snapshots.set(index, snapshot);
        }
        if (typeof data.from === 'string' && Array.isArray(data.rows)) {
          for (const edge of data.rows) edge.from ??= data.from;
        }
        onPage(data, index);
      },
    });
    const slices = new Map();
    // Return slices in their original row order, even across interleaved query fragments.
    for (const result of response.results) {
      const index = indexOf(result);
      if (!slices.has(index)) slices.set(index, []);
      slices.get(index).push(result.data);
    }
    return slices;
  }
  async function extras(data, first, rows, pageStart, index) {
    for (const [name, call] of Object.entries(data.next ?? {})) {
      if (name === 'retry' || name === 'restart') continue;
      if (call?.tool !== 'graphQuery' || !call.input) throw new Error(`Missing graph continuation for ${name}`);
      const count = first.summary?.[name];
      const list = listOf(name, first, rows, pageStart);
      const pages = await read(call.input, index);
      const tail = await collect(pages.get(index), index, list.length);
      for (const row of tail.rows) list.push(row);
      if (Number.isInteger(count) && list.length !== count) throw new Error(`Incomplete graph evidence list: ${name}`);
    }
  }
  // `shown`: rows of this list an earlier page already carried (a summary list or a cycle's first members).
  async function collect(slices, index, shown = 0) {
    if (!slices?.length) throw new Error(`Missing graph query result at index ${index}`);
    const first = slices[0];
    const totals = new Set(slices.map(data => data.pagination?.total).filter(Number.isInteger));
    if (totals.size > 1) throw new Error('Graph pagination.total changed within an evidence list');
    const [total] = totals;
    const rows = [];
    // A page without `rows` is a contract change, not an empty answer: fail rather than build an empty graph.
    for (const data of slices) {
      if (!Array.isArray(data.rows)) throw new Error(`graphQuery page has no rows array: ${Object.keys(data).join(',')}`);
      const pageStart = rows.length;
      for (const row of data.rows) rows.push(row);
      if (Number.isInteger(total) && shown + rows.length > total) throw new Error('Graph pages repeat rows: more rows than pagination.total');
      await extras(data, first, rows, pageStart, index);
    }
    if (Number.isInteger(total) && shown + rows.length !== total) throw new Error('Graph evidence ends without a required continuation');
    first.rows = rows;
    delete first.next;
    delete first.pagination;
    return first;
  }
  const pages = await read({ queries, responseChars: RESPONSE_CHARS });
  return Promise.all(queries.map((_, index) => collect(pages.get(index), index)));
}
