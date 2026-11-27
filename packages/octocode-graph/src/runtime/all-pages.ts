// `octocode-graph graphQuery '<json>' --all-pages`: one graphQuery call read to its end and printed once. readGraphs
// (graph-pages.ts) follows the reply's `next` until no page carries one, joins every `rowPart` fragment (tools.ts
// readResponse), concatenates each row's pages and merges every evidence list and cycle member list its `next.*`
// names, checking each list against its stated count. The printed reply has no `next`: every row once, with the
// fields of every page.
import { isDeepStrictEqual } from 'node:util';
import { isRecord } from './lib.ts';
import { graphCall, readGraphs, RESPONSE_CHARS } from './graph-pages.ts';
import type { GraphPage, ReadGraphOptions } from './graph-pages.ts';
import type { ToolReply, ToolResult } from './tools.ts';
import type { GraphQueryQuery } from '../contract.generated.js';

/** One query's answer: its index, the row fields its first page states (status, meta) and its joined data. */
export type JoinedResult = Omit<ToolResult, 'data' | 'rowPart'> & {
  index: number;
  data: GraphPage;
};

/** A graphQuery reply with every page read: its rows joined, and the envelope fields of every page. */
export interface JoinedReply {
  results: JoinedResult[];
  [field: string]: unknown;
}

/** An input the walk cannot start from: the CLI exits 2 on it, as for any other usage error. */
export class AllPagesInputError extends Error {}

// Envelope fields the joined reply leaves out: each page's rows and continuation, and `shared`, whose fields
// readResponse already put into every row they describe.
const PAGE_FIELDS = new Set([
  'results',
  'next',
  'expiresAt',
  'replayed',
  'shared',
]);

// Every page's envelope: an array (warnings, hints, related) gains each item no earlier page stated; any other field
// keeps the value the first page that states it gives.
function envelopeOf(pages: readonly ToolReply[]) {
  const out: Record<string, unknown> = {};
  for (const page of pages)
    for (const [key, value] of Object.entries(page)) {
      if (PAGE_FIELDS.has(key)) continue;
      const kept = out[key];
      if (Array.isArray(kept) && Array.isArray(value)) {
        for (const item of value)
          if (!kept.some(seen => isDeepStrictEqual(seen, item)))
            kept.push(item);
      } else if (!Object.hasOwn(out, key)) out[key] = structuredClone(value);
    }
  return out;
}

/** Reads a fresh graphQuery envelope to its end: every page, fragment and evidence list, each row once. */
export async function readAllPages(
  input: unknown,
  {
    run = graphCall,
    collectionPaths,
    collectionRefs,
  }: Pick<ReadGraphOptions, 'run' | 'collectionPaths' | 'collectionRefs'> = {}
): Promise<JoinedReply> {
  if (
    !isRecord(input) ||
    !Array.isArray(input.queries) ||
    !input.queries.length
  )
    throw new AllPagesInputError(
      isRecord(input) && 'cursor' in input
        ? '--all-pages starts from the fresh {"queries":[…]} call: a cursor continues a walk whose earlier pages this call never read'
        : '--all-pages needs a graphQuery envelope {"queries":[…]}'
    );
  const { queries, responseChars, ...other } = input;
  const unknown = Object.keys(other);
  if (unknown.length)
    throw new AllPagesInputError(
      `--all-pages reads queries and responseChars; not ${unknown.join(', ')}`
    );
  if (
    responseChars !== undefined &&
    (typeof responseChars !== 'number' ||
      !Number.isSafeInteger(responseChars) ||
      responseChars < 1)
  )
    throw new AllPagesInputError('responseChars must be a positive integer');
  const pages: ToolReply[] = [];
  const heads = new Map<number, Record<string, unknown>>();
  // The fresh call's own pages come first, each naming the next until the last; evidence lists are read after them.
  let chain = true;
  const answers = await readGraphs(queries as GraphQueryQuery[], {
    run,
    collectionPaths,
    collectionRefs,
    verbatim: true,
    responseChars: responseChars ?? RESPONSE_CHARS,
    save: (_input, raw) => {
      pages.push(raw);
      if (!chain) return;
      for (const result of raw.results ?? []) {
        const index = result.index ?? 0;
        if (heads.has(index)) continue;
        const { index: _i, data: _d, rowPart: _p, ...head } = result;
        heads.set(index, head);
      }
      chain = raw.next != null;
    },
  });
  return {
    results: answers.map((data, index) => ({
      index,
      ...heads.get(index),
      data,
    })),
    ...envelopeOf(pages),
  };
}

// The CLI's exit rule (`octocode --help`, "Exits:"): a batch exits with its strictest failed row's class, in this
// order, else 1 when every row is empty or out of range, else 0. A failed row without a known class is unservable.
const FAILED_EXIT: [string, number][] = [
  ['correctInput', 2],
  ['refused', 4],
  ['restart', 6],
  ['unservable', 5],
  ['transient', 7],
];
export const exitCodeOf = (errorClass: unknown) =>
  FAILED_EXIT.find(([name]) => name === errorClass)?.[1] ?? 5;

/** The exit code `octocode graphQuery` gives the same rows. */
export function replyExitCode(reply: JoinedReply) {
  const classes = reply.results
    .filter(r => r.status === 'error' || r.data.errorCode || r.data.error)
    .map((r): string =>
      FAILED_EXIT.some(([name]) => name === r.data.errorClass)
        ? String(r.data.errorClass)
        : 'unservable'
    );
  const strictest = FAILED_EXIT.find(([name]) => classes.includes(name));
  if (strictest) return strictest[1];
  return reply.results.length &&
    reply.results.every(r => r.status === 'empty' || r.status === 'outOfRange')
    ? 1
    : 0;
}
