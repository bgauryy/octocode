// Octocode tool calls followed to their end: the reply's top-level `next` until no reply carries one (it
// continues every unfinished row), then every row `next.*` branch of kind `continue`. Reassembled responses
// reach onPage without duplicate fragments.
import { isRecord, octocode } from './lib.ts';
import { isDeepStrictEqual } from 'node:util';
import type {
  GraphQueryData,
  GraphQueryOutputResults,
} from '../contract.generated.js';

/** A tool call: the tool and its input (a query envelope, or a continuation's own input). */
export interface ToolCall {
  tool: string;
  input: unknown;
}

/** A continuation as replies and rows carry it (the contract's `*ContinueCall`, whose `tool` is required; readers
 * still fall back to the calling tool). */
interface Continuation {
  kind?: string;
  tool: string;
  input?: unknown;
}

/** The fields every tool's row data shares (the contract states them once per tool); each tool adds its own. */
export type ToolData = Pick<
  GraphQueryData,
  | 'error'
  | 'errorCode'
  | 'errorClass'
  | 'isPartial'
  | 'partialReasons'
  | 'pagination'
  | 'warnings'
> & {
  next?: Record<string, Continuation | null | undefined>;
  [field: string]: unknown;
};

/** A part marker: which part of how many a row fragment is, and the string or array element it continues. */
type RowPart = NonNullable<
  GraphQueryOutputResults['results'][number]['rowPart']
>;

/** One result row of a reply: its query index, its data, and a part marker while the row continues. */
export interface ToolResult {
  index?: number;
  data?: ToolData;
  rowPart?: RowPart;
  status?: string;
  error?: string;
  errorCode?: string;
  errorClass?: string;
  isError?: unknown;
  [field: string]: unknown;
}

/** A tool reply: its result rows, row fields shared by every row on the page, its own continuation and errors. */
export interface ToolReply {
  results?: ToolResult[];
  shared?: { data?: unknown };
  next?: Continuation | null;
  status?: string;
  error?: string;
  errorCode?: string;
  errorClass?: string;
  isError?: unknown;
  [field: string]: unknown;
}

/** A reply read to its end: every row of every page joined. */
export type ReadReply = ToolReply & { results: ToolResult[] };

/** The output schema's collections: the array pointers whose fragments append, and references between them. */
export interface CollectionSchema {
  collectionPaths: string[];
  collectionRefs?: { path: string; target: string }[];
}

export type RunTool = (tool: string, input: unknown) => Promise<unknown>;

const runTool: RunTool = (tool, input) =>
  octocode([tool, '--json', JSON.stringify(input)], { partial: true });
const escapePointer = (key: string) =>
  key.replaceAll('~', '~0').replaceAll('/', '~1');
const pointerSteps = (pointer: string) =>
  pointer
    .split('/')
    .slice(1)
    .map(key => key.replaceAll('~1', '/').replaceAll('~0', '~'));
const outputSchemas = new Map<string, Promise<CollectionSchema>>();
function outputSchema(tool: string) {
  let schema = outputSchemas.get(tool);
  if (!schema) {
    schema = octocode(['schema', tool, '--view', 'output'])
      .then(answer => {
        // The output view is one JSON object: collectionPaths is checked here, collectionRefs where it is read.
        const view = answer as CollectionSchema;
        if (!Array.isArray(view.collectionPaths))
          throw new Error(
            `Octocode schema ${tool} does not expose collectionPaths; upgrade the CLI before reading response fragments`
          );
        return view;
      })
      .catch((error: unknown) => {
        outputSchemas.delete(tool);
        throw error;
      });
    outputSchemas.set(tool, schema);
  }
  return schema;
}
function collectionRoute(schema: CollectionSchema) {
  const paths = schema.collectionPaths.map(pointerSteps);
  const refs = (schema.collectionRefs ?? []).map(({ path, target }) => ({
    path: pointerSteps(path),
    target: pointerSteps(target),
  }));
  const matches = (pattern: string[], parts: string[]) =>
    pattern.length <= parts.length &&
    pattern.every((key, i) => key === '*' || key === parts[i]);
  return (path: string) => {
    let parts = pointerSteps(path);
    const seen = new Set<string>();
    for (;;) {
      if (
        paths.some(
          pattern => pattern.length === parts.length && matches(pattern, parts)
        )
      )
        return true;
      const key = JSON.stringify(parts);
      if (seen.has(key)) throw new Error('Cyclic collection schema reference');
      seen.add(key);
      const ref = refs.find(candidate => matches(candidate.path, parts));
      if (!ref) return false;
      if (ref.target.length >= ref.path.length)
        throw new Error('Collection schema reference does not reduce its path');
      parts = [...ref.target, ...parts.slice(ref.path.length)];
    }
  };
}

/** A part marker with the collection test of the tool's output schema. */
type Marker = RowPart & { collection: (path: string) => boolean };

// Later fragments repeat their identity, but carry only new evidence. The marker uses the
// joined row's coordinates; each array on the way is narrowed to its continuing element.
function mergePart(
  previous: unknown,
  fragment: unknown,
  marker: Marker,
  path = ''
): unknown {
  if (marker.text === path) return appendText(previous, fragment, path);
  if (Array.isArray(fragment))
    return mergeArray(previous, fragment, marker, path);
  if (fragment && typeof fragment === 'object')
    return mergeObject(previous, fragment, marker, path);
  if (previous !== undefined && previous !== fragment)
    throw new Error(`Repeated response fragment context disagrees at ${path}`);
  return fragment;
}

function appendText(previous: unknown, fragment: unknown, path: string) {
  if (typeof previous !== 'string' || typeof fragment !== 'string')
    throw new Error(`Response fragment text ${path} has no preceding string`);
  return previous + fragment;
}

// An array outside the schema's collections is repeated context: it must match what the first part carried.
function keepContext(previous: unknown[], fragment: unknown[], path: string) {
  if (!fragment.length || isDeepStrictEqual(previous, fragment))
    return previous;
  throw new Error(`Repeated response context disagrees at ${path}`);
}

// The pointer to the element a fragment continues: its text pointer, else its `continues` coordinates.
const continuedRoute = (marker: RowPart) =>
  marker.text ??
  (marker.continues
    ? `${marker.continues.array}/${marker.continues.index}`
    : undefined);

function continuedIndex(
  route: string,
  path: string,
  previous: unknown[],
  fragment: unknown[]
) {
  const at = Number(pointerSteps(route.slice(path.length))[0]);
  if (
    !Number.isSafeInteger(at) ||
    at < 0 ||
    at >= previous.length ||
    !fragment.length
  )
    throw new Error(
      `Response fragment continues a missing array element at ${path}`
    );
  return at;
}

// A collection appends every new element; the continued element merges with its first fragment element.
function mergeArray(
  previous: unknown,
  fragment: unknown[],
  marker: Marker,
  path: string
) {
  if (!Array.isArray(previous)) return structuredClone(fragment);
  if (!marker.collection(path)) return keepContext(previous, fragment, path);
  const route = continuedRoute(marker);
  if (route?.startsWith(`${path}/`)) {
    const at = continuedIndex(route, path, previous, fragment);
    previous[at] = mergePart(
      previous[at],
      fragment[0],
      marker,
      `${path}/${at}`
    );
    for (const item of structuredClone(fragment.slice(1))) previous.push(item);
  } else {
    for (const item of structuredClone(fragment)) previous.push(item);
  }
  return previous;
}

function mergeObject(
  previous: unknown,
  fragment: object,
  marker: Marker,
  path: string
) {
  const out: Record<string, unknown> = isRecord(previous) ? previous : {};
  for (const [key, value] of Object.entries(fragment)) {
    if (key === 'rowPart' && path === '') continue;
    Object.defineProperty(out, key, {
      value: mergePart(
        Object.hasOwn(out, key) ? out[key] : undefined,
        value,
        marker,
        `${path}/${escapePointer(key)}`
      ),
      writable: true,
      enumerable: true,
      configurable: true,
    });
  }
  return out;
}

// A JSON value's own field. Object.hasOwn reads a primitive's own properties too (a string's indexes).
function ownField(value: unknown, key: string): unknown {
  const object: Record<string, unknown> = Object(value ?? {});
  return Object.hasOwn(object, key) ? object[key] : undefined;
}

// Marker coordinates are in the joined row. In the incoming fragment each ancestor
// collection starts with its continued element.
function atPointer(previous: unknown, fragment: unknown, pointer: unknown) {
  if (typeof pointer !== 'string' || !pointer.startsWith('/'))
    throw new Error('Response fragment requires a JSON pointer');
  for (const key of pointerSteps(pointer)) {
    previous = ownField(previous, key);
    fragment = Array.isArray(fragment) ? fragment[0] : ownField(fragment, key);
  }
  return [previous, fragment];
}

/** A row joined so far: the merged row and the last part it holds. */
interface SavedPart {
  row: ToolResult;
  part: number;
  of: number;
}

// The saved part a fragment continues: none when it starts a row (part 1 after nothing or after a finished row).
const priorPart = (saved: SavedPart | undefined, marker: RowPart) =>
  saved?.part === saved?.of && marker.part === 1 ? undefined : saved;

function checkPartOrder(marker: RowPart, prior: SavedPart | undefined) {
  const { part, of } = marker;
  const counted =
    Number.isSafeInteger(part) &&
    Number.isSafeInteger(of) &&
    part >= 1 &&
    part <= of;
  const follows = prior
    ? part === prior.part + 1 && of === prior.of
    : part === 1;
  if (!counted || !follows)
    throw new Error('Response fragment parts are missing or out of order');
}

// Joins rowPart fragments per result index into `rows`, in arrival order. The output schema's
// collection paths are fetched once, only when a row actually continues.
function fragmentJoiner(tool: string, schema: CollectionSchema | undefined) {
  const parts = new Map<number | undefined, SavedPart>(),
    rows: ToolResult[] = [];
  let collection: ((path: string) => boolean) | undefined;
  async function joinPart(
    prior: SavedPart,
    row: ToolResult,
    marker: RowPart
  ): Promise<ToolResult> {
    collection ??= collectionRoute(schema ?? (await outputSchema(tool)));
    if (
      marker.text !== undefined &&
      !atPointer(prior.row, row, marker.text).every(v => typeof v === 'string')
    ) {
      throw new Error(
        `Response fragment text ${marker.text} names no string to continue`
      );
    }
    if (marker.continues !== undefined) {
      const { array, index } = marker.continues;
      const [before, after] = atPointer(prior.row, row, array);
      if (
        !Array.isArray(before) ||
        !Array.isArray(after) ||
        !after.length ||
        !collection(array) ||
        !Number.isSafeInteger(index) ||
        index < 0 ||
        index >= before.length
      ) {
        throw new Error(
          `Response fragment continues a missing collection element at ${array}`
        );
      }
    }
    // A row is an object: its parts merge field by field (atPointer has refused an empty text pointer).
    return mergeObject(prior.row, row, { ...marker, collection }, '');
  }
  async function add(row: ToolResult) {
    const marker = row.rowPart;
    if (!marker) {
      const whole = structuredClone(row);
      rows.push(whole);
      return whole;
    }
    const prior = priorPart(parts.get(row.index), marker);
    checkPartOrder(marker, prior);
    const joined = prior
      ? await joinPart(prior, row, marker)
      : structuredClone(row);
    delete joined.rowPart;
    if (!prior) rows.push(joined);
    parts.set(row.index, { row: joined, part: marker.part, of: marker.of });
    return marker.part === marker.of ? joined : undefined;
  }
  function finish() {
    if ([...parts.values()].some(({ part, of }) => part !== of))
      throw new Error('Response fragment ends before its final part');
  }
  return { rows, add, finish };
}

// The reply's one continuation; null once no reply carries it: the answer is then read to its end.
function nextResponseCall(page: ToolReply, call: ToolCall): ToolCall | null {
  const next = page.next;
  if (next == null) return null;
  if (!next.input)
    throw new Error(`Missing ${call.tool} response continuation input`);
  return { tool: next.tool ?? call.tool, input: next.input };
}

// A batch can state nested fields common to all rows in shared.data on that page.
function withShared(shared: unknown, own: unknown): unknown {
  if (own === undefined) return shared;
  if (!isRecord(shared) || !isRecord(own)) return own;
  const inherited = Object.entries(shared).map(([key, value]) => [
    key,
    withShared(value, Object.hasOwn(own, key) ? own[key] : undefined),
  ]);
  return { ...own, ...Object.fromEntries(inherited) };
}

export interface ReadOptions {
  run?: RunTool;
  onPage?: (page: ToolReply, call: ToolCall) => void;
  onResult?: (result: ToolResult) => void;
  collectionPaths?: string[];
  collectionRefs?: CollectionSchema['collectionRefs'];
}

// Follow the reply's `next` to its end before following any row branch. All consumers
// share this join so long strings and nested rows are never mistaken for extra results.
// Each later page of a row keeps the row's index and remains a separate slice.
export async function readResponse(
  tool: string,
  input: unknown,
  {
    run = runTool,
    onPage = () => {},
    onResult,
    collectionPaths,
    collectionRefs,
  }: ReadOptions = {}
): Promise<ReadReply> {
  const seen = new Set<string>();
  const fragments = fragmentJoiner(
    tool,
    collectionPaths ? { collectionPaths, collectionRefs } : undefined
  );
  let call: ToolCall | null = { tool, input },
    combined: ReadReply | undefined;
  // The first page is always read; it sets the envelope.
  do {
    const key = JSON.stringify(call);
    if (seen.has(key))
      throw new Error(
        `Repeated ${call.tool} response continuation; evidence is incomplete`
      );
    seen.add(key);
    // A tool prints one JSON object per page: its error fields and its results array are checked before any row is read.
    const page = (await run(call.tool, call.input)) as ToolReply;
    onPage(page, call);
    if (
      page.errorCode ||
      page.error ||
      page.isError ||
      page.status === 'error'
    ) {
      throw Object.assign(
        new Error(
          `${call.tool} ${page.errorCode ?? 'error'}: ${page.error ?? 'tool result failed'}`
        ),
        { errorClass: page.errorClass }
      );
    }
    if (!Array.isArray(page.results))
      throw new Error(`${call.tool} response is missing its results array`);
    if (!combined) {
      // The joiner already copies each result. Only the first page's envelope is needed here.
      const { results, ...header } = page;
      combined = { ...structuredClone(header), results: fragments.rows };
    }
    for (const row of page.results) {
      // Shared fields only add to the row's own data object.
      const complete = await fragments.add(
        page.shared?.data && row.data
          ? { ...row, data: withShared(page.shared.data, row.data) as ToolData }
          : row
      );
      if (complete) onResult?.(complete);
    }
    call = nextResponseCall(page, call);
  } while (call);
  fragments.finish();
  for (const key of ['next', 'expiresAt', 'replayed']) delete combined[key];
  return combined;
}

// Continuations a response hands back: the reply's own `next` first (TOOL_DATA_CONTRACT), then each row's branches.
const rowContinuations = (row: ToolResult | undefined, tool?: string) =>
  Object.values(row?.data?.next ?? {})
    .filter(
      (c): c is Continuation => c?.kind === 'continue' && Boolean(c.input)
    )
    .map(c => ({ tool: c.tool ?? tool, input: c.input }));
export function continuations(page: ToolReply | undefined, tool?: string) {
  const reply = page?.next;
  if (reply?.input) return [{ tool: reply.tool ?? tool, input: reply.input }];
  return (page?.results ?? []).flatMap(row => rowContinuations(row, tool));
}

export interface PagedOptions {
  onPage?: (page: ReadReply, n: number, call: ToolCall) => void;
  run?: RunTool;
  collectionPaths?: string[];
  collectionRefs?: CollectionSchema['collectionRefs'];
}

// Runs `input` and every continuation it emits; returns the pages in order. A repeated cursor is a defect.
export async function pagedTool(
  tool: string,
  input: unknown,
  {
    onPage = () => {},
    run = runTool,
    collectionPaths,
    collectionRefs,
  }: PagedOptions = {}
) {
  const pages: ReadReply[] = [];
  const seen = new Set<string>();
  const queue: ToolCall[] = [{ tool, input }];
  while (queue.length) {
    // The loop runs only while the queue holds a call.
    const call = queue.shift()!;
    const key = JSON.stringify(call);
    if (seen.has(key))
      throw new Error(
        `Repeated ${call.tool} continuation; evidence is incomplete`
      );
    seen.add(key);
    const page = await readResponse(call.tool, call.input, {
      run,
      collectionPaths,
      collectionRefs,
    });
    const failure = page.results.find(
      value =>
        value.errorCode ||
        value.error ||
        value.isError ||
        value.status === 'error' ||
        value.data?.errorCode ||
        value.data?.error
    );
    if (failure) {
      const detail = failure.data ?? failure;
      throw Object.assign(
        new Error(
          `${call.tool} ${detail.errorCode ?? 'error'}: ${detail.error ?? 'tool result failed'}; discard this evidence walk before restarting`
        ),
        { errorClass: detail.errorClass }
      );
    }
    pages.push(page);
    onPage(page, pages.length, call);
    queue.unshift(...continuations(page, call.tool));
  }
  return pages;
}

// Rows of every page for one result field (e.g. `symbols`, `files`), in page order. `Row` is the row shape the tool
// documents for that field; rows pass through unchecked.
export const rowsOf = <Row = unknown>(
  pages: readonly ToolReply[],
  field: string
) =>
  pages.flatMap(p =>
    (p.results ?? []).flatMap(r => r?.data?.[field] ?? [])
  ) as Row[];

/** The query fields a tool call summary names. */
interface CallQuery {
  operation?: string;
  view?: string;
  target?: string | string[];
  path?: string;
  kind?: string;
  edges?: string[];
  symbolName?: string;
}

// One line summarizing a tool call for the viewer's Tools tab: operation, target, first page or cursor.
export function describeCall(args: string[]) {
  const [tool, ...rest] = args;
  const json = rest.find(a => a.startsWith('{'));
  let input: { cursor?: string; queries?: CallQuery[] } = {};
  try {
    input = json ? JSON.parse(json) : {};
  } catch {
    /* the call is still logged with its raw args */
  }
  if (input.cursor)
    return {
      tool,
      detail: 'cursor',
      page: `cursor ${input.cursor.slice(-10)}`,
    };
  const q = input.queries ?? [];
  if (q.length <= 1 && (tool === 'graphBuild' || tool === 'graphQuery')) {
    const x = q[0] ?? {};
    const targets = [x.target].flat().filter(Boolean);
    const target =
      targets.length > 1 ? `${targets.length} targets` : targets[0];
    const what =
      tool === 'graphBuild' ? x.path?.split('/').slice(-2).join('/') : target;
    return {
      tool,
      detail: [x.operation, what, x.kind, x.edges?.join(',')]
        .filter(Boolean)
        .join(' · '),
      page: 'first',
    };
  }
  const ops = [...new Set(q.map(x => x.operation ?? x.view ?? ''))]
    .filter(Boolean)
    .join(',');
  const what = q
    .map(x =>
      [x.symbolName, x.path?.split('/').pop()].filter(Boolean).join('@')
    )
    .slice(0, 2)
    .join('; ');
  return {
    tool,
    detail: [ops, what, q.length > 1 ? `${q.length} queries` : '']
      .filter(Boolean)
      .join(' · '),
    page: 'first',
  };
}

// Grouped rows (structure `{dir, files}`, LSP `{path, matches}`) count their members.
const groupSize = (x: unknown) => {
  const group = isRecord(x) ? x : {};
  return Array.isArray(group.files)
    ? group.files.length
    : Array.isArray(group.matches)
      ? group.matches.length
      : 1;
};
const ROW_FIELDS = ['files', 'symbols', 'matches', 'rows'];
const rowCount = (d: ToolData) =>
  ROW_FIELDS.reduce((sum, k) => {
    const rows = d[k];
    return (
      sum +
      (Array.isArray(rows)
        ? rows.reduce((a: number, x) => a + groupSize(x), 0)
        : 0)
    );
  }, 0);

// Rows a response carries (the row fields of tool results, graph `rows` among them) and whether it continues.
export function responseShape(value: unknown) {
  if (!value || typeof value !== 'object') return { rows: 0, more: false };
  // An object answer is a reply envelope: each field is read optionally.
  const reply = value as ToolReply;
  const datas = (reply.results ?? []).map(r => r?.data ?? {});
  const rows = datas.reduce((sum, d) => sum + rowCount(d), 0);
  const total = datas.length === 1 ? datas[0].pagination?.total : undefined;
  return {
    rows,
    ...(Number.isInteger(total) ? { total } : {}),
    more: continuations(reply).length > 0,
    error: datas.find(d => d.errorCode)?.errorCode,
  };
}
