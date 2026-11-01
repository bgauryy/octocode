import { execFileSync } from 'node:child_process';
import { cliCommand, octocode } from './lib.mjs';

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

// The rows a graphQuery row holds in all: `pagination.totalItems`, which a complete single page omits (its rows
// are then the whole answer).
export function totalOf(data) {
  const total = data.pagination?.totalItems ?? (data.next?.nextPage ? undefined : data.rows?.length);
  return Number.isInteger(total) ? total : undefined;
}

// The exact total of a graphQuery row from its first page alone (no walk).
export async function graphTotal(row, { run = graphCall } = {}) {
  const total = totalOf(dataOf('graphQuery', await run('graphQuery', envelope({ ...row, limit: 1 }))));
  if (total === undefined) throw new Error(`graphQuery ${row.operation} returned no total`);
  return total;
}

// A build's stderr progress lines (`graphBuild: parsed 120/400 files`) reach `onProgress` without the tool prefix.
export async function graphBuild(row, { run = graphCall, onProgress = () => {} } = {}) {
  const onStderr = (line) => onProgress(line.replace(/^graphBuild: /, ''));
  return dataOf('graphBuild', await run('graphBuild', envelope(row), { onStderr }));
}

// The continuation of a response, in the order the contract reads them: the serialized-response page first,
// then the row's own `next.nextPage`. List and cycle-member continuations are read separately.
const pageContinuation = (response, data) => response?.responsePagination?.next?.input ?? data.next?.nextPage?.input;

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

// Every row of one graphQuery row, read to its end: `next.nextPage` pages, every `summary.*`/`baseline.*` evidence
// list (`next['summary.x']`) and every cycle's member list (`next.cycle<i>`, i = the row on its page). Raw pages
// reach save() before they are joined. Pages carry no row offset (it lives in the cursor), so exactly-once is
// checked by count: a list's rows, the ones already shown included, must end equal to its `totalItems`. A repeated
// input, a count mismatch or a missing continuation rejects: the result is complete or there is none. The result
// is the first page's data with `rows` holding every row and no `pagination` or `next`.
export async function readGraph(row, { run = graphCall, save = () => {} } = {}) {
  const seen = new Set();
  async function read(input) {
    const key = JSON.stringify(input);
    if (seen.has(key)) throw new Error('Repeated graph continuation; evidence is incomplete');
    seen.add(key);
    const response = await run('graphQuery', input);
    save(input, response);
    return { response, data: structuredClone(dataOf('graphQuery', response)) };
  }
  async function extras(data, first, rows, pageStart) {
    for (const [name, call] of Object.entries(data.next ?? {})) {
      if (name === 'nextPage' || call?.kind !== 'continue') continue;
      const list = listOf(name, first, rows, pageStart);
      const tail = await collect(call.input, list.length);
      list.push(...tail.rows);
    }
  }
  // `shown`: rows of this list an earlier page already carried (a summary list or a cycle's first members).
  async function collect(input, shown = 0) {
    let { response, data } = await read(input);
    const first = data;
    const total = data.pagination?.totalItems;
    const rows = [];
    for (;;) {
      // A page without `rows` is a contract change, not an empty answer: fail rather than build an empty graph.
      if (!Array.isArray(data.rows)) throw new Error(`graphQuery page has no rows array: ${Object.keys(data).join(',')}`);
      const pageStart = rows.length;
      rows.push(...data.rows);
      if (Number.isInteger(total) && shown + rows.length > total) throw new Error('Graph pages repeat rows: more rows than totalItems');
      await extras(data, first, rows, pageStart);
      const next = pageContinuation(response, data);
      if (!next) break;
      ({ response, data } = await read(next));
    }
    if (Number.isInteger(total) && shown + rows.length !== total) throw new Error('Graph evidence ends without a required continuation');
    first.rows = rows;
    delete first.next;
    delete first.pagination;
    return first;
  }
  return collect(envelope(row));
}
