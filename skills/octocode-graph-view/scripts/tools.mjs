// Octocode tool calls followed to their end: responsePagination.next first, then every `next.*` continuation
// of kind `continue` (one chain per result row). Reassembled responses reach onPage without duplicate fragments.
import { octocode } from './lib.mjs';
import { isDeepStrictEqual } from 'node:util';

const runTool = (tool, input) => octocode([tool, '--json', JSON.stringify(input)], { partial: true });
const escapePointer = (key) => String(key).replaceAll('~', '~0').replaceAll('/', '~1');
const pointerSteps = (pointer) => pointer.split('/').slice(1).map((key) => key.replaceAll('~1', '/').replaceAll('~0', '~'));
const outputSchemas = new Map();
function outputSchema(tool) {
  if (!outputSchemas.has(tool)) outputSchemas.set(tool, octocode(['schema', tool, '--view', 'output']).then((schema) => {
    if (!Array.isArray(schema.collectionPaths)) throw new Error(`Octocode schema ${tool} does not expose collectionPaths; upgrade the CLI before reading response fragments`);
    return schema;
  }).catch((error) => { outputSchemas.delete(tool); throw error; }));
  return outputSchemas.get(tool);
}
function collectionRoute(schema) {
  const paths = schema.collectionPaths.map(pointerSteps);
  const refs = (schema.collectionRefs ?? []).map(({path,target}) => ({path:pointerSteps(path),target:pointerSteps(target)}));
  const matches = (pattern, parts) => pattern.length <= parts.length && pattern.every((key,i) => key === '*' || key === parts[i]);
  return (path) => {
    let parts = pointerSteps(path);
    const seen = new Set();
    for (;;) {
      if (paths.some(pattern => pattern.length === parts.length && matches(pattern,parts))) return true;
      const key = JSON.stringify(parts);
      if (seen.has(key)) throw new Error('Cyclic collection schema reference');
      seen.add(key);
      const ref = refs.find(candidate => matches(candidate.path,parts));
      if (!ref) return false;
      if (ref.target.length >= ref.path.length) throw new Error('Collection schema reference does not reduce its path');
      parts = [...ref.target,...parts.slice(ref.path.length)];
    }
  };
}

// Later fragments repeat their identity, but carry only new evidence. The marker uses the
// joined row's coordinates; each array on the way is narrowed to its continuing element.
function mergePart(previous, fragment, marker, path = '') {
  if (marker.text === path) return appendText(previous, fragment, path);
  if (Array.isArray(fragment)) return mergeArray(previous, fragment, marker, path);
  if (fragment && typeof fragment === 'object') return mergeObject(previous, fragment, marker, path);
  return previous === undefined ? fragment : previous;
}

function appendText(previous, fragment, path) {
  if (typeof previous !== 'string' || typeof fragment !== 'string') throw new Error(`Response fragment text ${path} has no preceding string`);
  return previous + fragment;
}

// An array outside the schema's collections is repeated context: it must match what the first part carried.
function keepContext(previous, fragment, path) {
  if (!fragment.length || isDeepStrictEqual(previous,fragment)) return previous;
  throw new Error(`Repeated response context disagrees at ${path}`);
}

// The pointer to the element a fragment continues: its text pointer, else its `continues` coordinates.
const continuedRoute = (marker) => marker.text ?? (marker.continues ? `${marker.continues.array}/${marker.continues.index}` : undefined);

function continuedIndex(route, path, previous, fragment) {
  const at = Number(pointerSteps(route.slice(path.length))[0]);
  if (!Number.isSafeInteger(at) || at < 0 || at >= previous.length || !fragment.length) throw new Error(`Response fragment continues a missing array element at ${path}`);
  return at;
}

// A collection appends every new element; the continued element merges with its first fragment element.
function mergeArray(previous, fragment, marker, path) {
  if (!Array.isArray(previous)) return structuredClone(fragment);
  if (!marker.collection(path)) return keepContext(previous, fragment, path);
  const route = continuedRoute(marker);
  if (route?.startsWith(`${path}/`)) {
    const at = continuedIndex(route, path, previous, fragment);
    previous[at] = mergePart(previous[at], fragment[0], marker, `${path}/${at}`);
    previous.push(...structuredClone(fragment.slice(1)));
  } else previous.push(...structuredClone(fragment));
  return previous;
}

function mergeObject(previous, fragment, marker, path) {
  const out = previous && typeof previous === 'object' && !Array.isArray(previous) ? previous : {};
  for (const [key, value] of Object.entries(fragment)) {
    if (key === 'rowPart' && path === '') continue;
    Object.defineProperty(out, key, { value: mergePart(Object.hasOwn(out, key) ? out[key] : undefined, value, marker, `${path}/${escapePointer(key)}`), writable: true, enumerable: true, configurable: true });
  }
  return out;
}

function validateText(previous, fragment, pointer) {
  if (typeof pointer !== 'string' || !pointer.startsWith('/')) throw new Error('Response fragment text requires a JSON pointer');
  for (const key of pointerSteps(pointer)) {
    previous = previous?.[key];
    fragment = Array.isArray(fragment) ? fragment[0] : fragment?.[key];
  }
  if (typeof previous !== 'string' || typeof fragment !== 'string') throw new Error(`Response fragment text ${pointer} names no string to continue`);
}

// The saved part a fragment continues: none when it starts a row (part 1 after nothing or after a finished row).
const priorPart = (saved, marker) => (saved?.part === saved?.of && marker.part === 1 ? undefined : saved);

function checkPartOrder(marker, prior) {
  const { part, of } = marker;
  const counted = Number.isSafeInteger(part) && Number.isSafeInteger(of) && part >= 1 && part <= of;
  const follows = prior ? part === prior.part + 1 && of === prior.of : part === 1;
  if (!counted || !follows) throw new Error('Response fragment parts are missing or out of order');
}

// Joins rowPart fragments per result index into `rows`, in arrival order. The output schema's
// collection paths are fetched once, only when a row actually continues.
function fragmentJoiner(tool, schema) {
  const parts = new Map(), rows = [];
  let collection;
  async function joinPart(prior, row, marker) {
    collection ??= collectionRoute(schema ?? await outputSchema(tool));
    if (marker.text !== undefined) validateText(prior.row,row,marker.text);
    return mergePart(prior.row, row, {...marker,collection});
  }
  async function add(row) {
    const marker = row.rowPart;
    if (!marker) { rows.push(structuredClone(row)); return; }
    const prior = priorPart(parts.get(row.index), marker);
    checkPartOrder(marker, prior);
    const joined = prior ? await joinPart(prior, row, marker) : structuredClone(row);
    delete joined.rowPart;
    if (!prior) rows.push(joined);
    parts.set(row.index, { row: joined, part: marker.part, of: marker.of });
  }
  function finish() {
    if ([...parts.values()].some(({ part, of }) => part !== of)) throw new Error('Response fragment ends before its final part');
  }
  return { rows, add, finish };
}

// The serialized response's next page; null once it is complete. hasMore without a call is a defect.
function nextResponseCall(page, call) {
  const next = page.responsePagination?.next;
  if (next?.input) return { tool: next.tool ?? call.tool, input: next.input };
  if (page.responsePagination?.hasMore) throw new Error('Response evidence ends without a required continuation');
  return null;
}

// The last row per query index states whether that query continues; a continuing row must carry its call.
function requireRowContinuations(rows, tool) {
  const last = new Map(rows.map((row) => [row.index,row]));
  for (const row of last.values()) if ((row.data?.pagination?.hasMore || row.data?.hasMore) && !rowContinuations(row,tool).length) throw new Error(`${tool} evidence ends without a required continuation`);
}

// Finish the serialized response before following any tool-level branch. All consumers
// share this join so long strings and nested rows are never mistaken for extra results.
// Complete slices from a hoisted tool walk keep the same index and remain separate.
export async function readResponse(tool, input, { run = runTool, onPage = () => {}, collectionPaths, collectionRefs } = {}) {
  const seen = new Set();
  const fragments = fragmentJoiner(tool, collectionPaths ? {collectionPaths,collectionRefs} : undefined);
  let call = { tool, input }, combined;
  while (call) {
    const key = JSON.stringify(call);
    if (seen.has(key)) throw new Error(`Repeated ${call.tool} response continuation; evidence is incomplete`);
    seen.add(key);
    const page = await run(call.tool, call.input);
    onPage(page, call);
    combined ??= { ...structuredClone(page), results: fragments.rows };
    for (const row of page.results ?? []) await fragments.add(row);
    call = nextResponseCall(page, call);
  }
  fragments.finish();
  requireRowContinuations(fragments.rows, tool);
  delete combined.responsePagination;
  return combined;
}

// Continuations a response hands back: the serialized-response page first (TOOL_DATA_CONTRACT), then each row's.
const rowContinuations = (row, tool) => Object.values(row?.data?.next ?? {}).filter((c) => c?.kind === 'continue' && c.input).map((c) => ({ tool: c.tool ?? tool, input: c.input }));
export function continuations(page, tool) {
  const rp = page?.responsePagination?.next;
  if (rp?.input) return [{ tool: rp.tool ?? tool, input: rp.input }];
  return (page?.results ?? []).flatMap((row) => rowContinuations(row, tool));
}

// Runs `input` and every continuation it emits; returns the pages in order. A repeated cursor is a defect.
export async function pagedTool(tool, input, { onPage = () => {}, run = runTool, collectionPaths, collectionRefs } = {}) {
  const pages = [];
  const seen = new Set();
  const queue = [{ tool, input }];
  while (queue.length) {
    const call = queue.shift();
    const key = JSON.stringify(call);
    if (seen.has(key)) throw new Error(`Repeated ${call.tool} continuation; evidence is incomplete`);
    seen.add(key);
    const page = await readResponse(call.tool, call.input, { run, collectionPaths, collectionRefs });
    const failure = [page, ...(page.results ?? [])].find((value) => value.errorCode || value.error || value.isError || value.status === 'error' || value.data?.errorCode || value.data?.error);
    if (failure) {
      const detail = failure.data ?? failure;
      throw new Error(`${call.tool} ${detail.errorCode ?? 'error'}: ${detail.error ?? 'tool result failed'}; discard this evidence walk before restarting`);
    }
    pages.push(page);
    onPage(page, pages.length, call);
    queue.unshift(...continuations(page, call.tool));
  }
  return pages;
}

// Rows of every page for one result field (e.g. `symbols`, `files`), in page order.
export const rowsOf = (pages, field) => pages.flatMap((p) => (p.results ?? []).flatMap((r) => r?.data?.[field] ?? []));

// One line summarizing a tool call for the viewer's Tools tab: operation, target, first page or cursor.
export function describeCall(args) {
  const [tool, ...rest] = args;
  const json = rest.find((a) => a.startsWith('{'));
  let input = {};
  try { input = json ? JSON.parse(json) : {}; } catch { /* the call is still logged with its raw args */ }
  if (input.cursor) return { tool, detail: 'cursor', page: `cursor ${String(input.cursor).slice(-10)}` };
  const q = input.queries ?? [];
  if (tool === 'graphBuild' || tool === 'graphQuery') {
    const x = q[0] ?? {};
    const targets = [x.target].flat().filter(Boolean);
    const target = targets.length > 1 ? `${targets.length} targets` : targets[0];
    const what = tool === 'graphBuild' ? x.path?.split('/').slice(-2).join('/') : target;
    return { tool, detail: [x.operation, what, x.kind, x.edges?.join(',')].filter(Boolean).join(' · '), page: 'first' };
  }
  const ops = [...new Set(q.map((x) => x.operation ?? x.view ?? ''))].filter(Boolean).join(',');
  const what = q.map((x) => [x.symbolName, x.path?.split('/').pop()].filter(Boolean).join('@')).slice(0, 2).join('; ');
  return { tool, detail: [ops, what, q.length > 1 ? `${q.length} queries` : ''].filter(Boolean).join(' · '), page: 'first' };
}

// Grouped rows (structure `{dir, files}`, LSP `{path, matches}`) count their members.
const groupSize = (x) => (Array.isArray(x?.files) ? x.files.length : Array.isArray(x?.matches) ? x.matches.length : 1);
const ROW_FIELDS = ['files', 'symbols', 'matches', 'rows'];
const rowCount = (d) => ROW_FIELDS.reduce((sum, k) => sum + (Array.isArray(d[k]) ? d[k].reduce((a, x) => a + groupSize(x), 0) : 0), 0);

// Rows a response carries (the row fields of tool results, graph `rows` among them) and whether it continues.
export function responseShape(value) {
  if (!value || typeof value !== 'object') return { rows: 0, more: false };
  const datas = (value.results ?? []).map((r) => r?.data ?? {});
  const rows = datas.reduce((sum, d) => sum + rowCount(d), 0);
  const total = datas.length === 1 ? datas[0].pagination?.total : undefined;
  return { rows, ...(Number.isInteger(total) ? { total } : {}), more: continuations(value).length > 0, error: datas.find((d) => d.errorCode)?.errorCode };
}
