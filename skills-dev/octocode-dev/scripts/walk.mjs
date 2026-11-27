#!/usr/bin/env node
// Walks a tool reply's top-level `next` through the CLI to its end and checks the continuation contract
// (docs/TOOL_DATA_CONTRACT.md#executable-continuations): the walk terminates, no cursor or page repeats, and every
// query index rebuilds from its row parts exactly once. Row parts are joined by the output schema's collection
// paths (`octocode schema <tool> --view output`), as a client joins them.
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { isDeepStrictEqual } from 'node:util';
import { isMainModule } from './lib/entrypoint.mjs';
import { ROOT } from './lib/workspaces.mjs';

export const CLI = path.join(ROOT, 'packages', 'octocode', 'out', 'octocode.js');

const HELP = `Usage: node skills-dev/octocode-dev/scripts/walk.mjs <tool> '<json>' [--bin <octocode binary>] [--max-pages N] [--out <file>]

Runs \`node packages/octocode/out/octocode.js <tool> <json> --json\` (or \`<bin> <tool> <json> --json\`, e.g. a
\`dev pin\` path), then sends each reply's top-level next.input to next.tool until a reply has none. Row parts
(rowPart) are joined per query index with the tool's output collectionPaths, as a client joins them.

Prints {ok, tool, pages, replyChars, rows, partial, rowErrors, violations}:
  rows[]       per query index: slices (rebuilt rows; a row page after the first is a later slice), parts,
               and collections (items per output collection path, summed over slices)
  partial[]    rows that state isPartial, partialReasons or terminalLimit (disclosed limits, not violations)
  rowErrors[]  rows that carry an error (an isolated row failure, not a violation)
  violations[] {code, page, index?, message}; codes:
    notTerminated     --max-pages reached with a next still pending (the pending call is in the message)
    repeatedCursor    a continuation input was already sent
    repeatedPage      a reply's results equal an earlier page's
    noProgress        a reply with a next carries no result row
    badPage           stdout is not one JSON reply
    pageError         a continuation page failed as a whole (errorCode in the message)
    badContinuation   next without input, or a rowPart.text/continues pointer that names nothing
    partOrder         a row part is missing, repeated or out of order
    unfinishedRow     the walk ended inside a row's parts
    contextDisagrees  a repeated field of a row's parts changed between parts
    repeatedEvidence  an item appears twice in one rebuilt collection
    paginationGap, paginationOverlap, totalChanged, paginationShort
                      a row's text-window pages (data.pagination offset/length) skip, repeat, restate a
                      different total, or stop short of it without a partial disclosure
    missingIndex, unexpectedIndex   a query index with no row, or a row for no query
  --max-pages N  stop after N pages (default: none; repeats end the walk)
  --out <file>   also write the rebuilt reply {tool, input, results} (one entry per slice) to <file>

Errors print {ok:false,error:{code,message}}: invalidInput (the first call is refused as correctInput, or bad
arguments), noCli (the CLI is not built: run \`dev build:dev\`).
Exit codes: 0 every check passed; 1 a violation or a failed run; 2 invalid input.`;

class WalkError extends Error {
  constructor(code, message, exitCode) {
    super(message);
    this.code = code;
    this.exitCode = exitCode;
  }
}

/** A violation found while joining one row part. */
class Violation extends Error {
  constructor(code, message) {
    super(message);
    this.code = code;
  }
}

/** Runs the CLI (or `bin`) with `args`; returns {status, stdout, stderr}. */
export function cliRunner({ bin } = {}) {
  if (!bin && !fs.existsSync(CLI)) throw new WalkError('noCli', `${CLI} is not built; run \`dev build:dev\``, 1);
  const [command, prefix] = bin ? [bin, []] : [process.execPath, [CLI]];
  return args => {
    const ran = spawnSync(command, [...prefix, ...args], { encoding: 'utf8', maxBuffer: 2 ** 31, stdio: ['ignore', 'pipe', 'pipe'] });
    if (ran.error) throw new WalkError('runFailed', `${command} failed to start: ${ran.error.message}`, 1);
    return { status: ran.status, stdout: ran.stdout, stderr: ran.stderr };
  };
}

const isRecord = value => Boolean(value) && typeof value === 'object' && !Array.isArray(value);
const escapePointer = key => String(key).replaceAll('~', '~0').replaceAll('/', '~1');
const pointerSteps = pointer => pointer.split('/').slice(1).map(key => key.replaceAll('~1', '/').replaceAll('~0', '~'));

/** `pointer → is it a collection` from collectionPaths (`*` matches one step) and collectionRefs. */
export function collectionRoute({ collectionPaths = [], collectionRefs = [] }) {
  const paths = collectionPaths.map(pointerSteps);
  const refs = collectionRefs.map(({ path: from, target }) => ({ path: pointerSteps(from), target: pointerSteps(target) }));
  const matches = (pattern, parts) => pattern.length <= parts.length && pattern.every((key, i) => key === '*' || key === parts[i]);
  return pointer => {
    let parts = pointerSteps(pointer);
    for (let hops = 0; hops <= refs.length; hops += 1) {
      if (paths.some(pattern => pattern.length === parts.length && matches(pattern, parts))) return true;
      const ref = refs.find(candidate => matches(candidate.path, parts));
      if (!ref || ref.target.length >= ref.path.length) return false;
      parts = [...ref.target, ...parts.slice(ref.path.length)];
    }
    return false;
  };
}

/** Shared facts fill only the fields a row lacks (TOOL_DATA_CONTRACT: shared facts). */
function withShared(shared, own) {
  if (own === undefined) return structuredClone(shared);
  if (!isRecord(shared) || !isRecord(own)) return own;
  const out = { ...own };
  for (const [key, value] of Object.entries(shared)) out[key] = withShared(value, Object.hasOwn(own, key) ? own[key] : undefined);
  return out;
}

function ownField(value, key) {
  const object = Object(value ?? {});
  return Object.hasOwn(object, key) ? object[key] : undefined;
}

/** The values at `pointer` in the joined row and in the incoming part (each array on the way starts with its continued element). */
function atPointer(joined, part, pointer) {
  if (typeof pointer !== 'string' || !pointer.startsWith('/')) throw new Violation('badContinuation', `rowPart pointer ${JSON.stringify(pointer)} is not a JSON pointer`);
  for (const key of pointerSteps(pointer)) {
    joined = ownField(joined, key);
    part = Array.isArray(part) ? part[0] : ownField(part, key);
  }
  return [joined, part];
}

/** Merges one row part into the joined row: collections append (a continued element merges), text appends at
 * rowPart.text, and anything else repeats unchanged. */
function mergePart(previous, part, marker, at) {
  if (marker.text === at) {
    if (typeof previous !== 'string' || typeof part !== 'string') throw new Violation('badContinuation', `rowPart.text ${at} names no string to continue`);
    return previous + part;
  }
  if (Array.isArray(part)) {
    if (!Array.isArray(previous)) return structuredClone(part);
    if (!marker.collection(at)) {
      if (!part.length || isDeepStrictEqual(previous, part)) return previous;
      throw new Violation('contextDisagrees', `${at} is not a collection and differs between parts`);
    }
    const route = marker.text ?? (marker.continues ? `${marker.continues.array}/${marker.continues.index}` : undefined);
    let rest = part;
    if (route?.startsWith(`${at}/`)) {
      const index = Number(pointerSteps(route.slice(at.length))[0]);
      if (!Number.isSafeInteger(index) || index < 0 || index >= previous.length || !part.length) {
        throw new Violation('badContinuation', `rowPart continues a missing element of ${at}`);
      }
      previous[index] = mergePart(previous[index], part[0], marker, `${at}/${index}`);
      rest = part.slice(1);
    }
    for (const item of structuredClone(rest)) previous.push(item);
    return previous;
  }
  if (isRecord(part)) {
    const out = isRecord(previous) ? previous : {};
    for (const [key, value] of Object.entries(part)) {
      if (key === 'rowPart' && at === '') continue;
      out[key] = mergePart(Object.hasOwn(out, key) ? out[key] : undefined, value, marker, `${at}/${escapePointer(key)}`);
    }
    return out;
  }
  if (previous !== undefined && previous !== part) {
    throw new Violation('contextDisagrees', `${at} is ${JSON.stringify(previous)} in an earlier part and ${JSON.stringify(part)} in this one`);
  }
  return part;
}

/** Visits every array under `value` with its pointer. */
function eachArray(value, at, visit) {
  if (Array.isArray(value)) {
    visit(value, at);
    value.forEach((item, i) => eachArray(item, `${at}/${i}`, visit));
  } else if (isRecord(value)) {
    for (const [key, item] of Object.entries(value)) eachArray(item, `${at}/${escapePointer(key)}`, visit);
  }
}

const patternOf = pointer => pointer.replace(/\/\d+(?=\/|$)/g, '/*');
const partialOf = data => isRecord(data) && (data.isPartial === true || data.partialReasons !== undefined || data.terminalLimit !== undefined);

/**
 * Walks `input` to its end. `run(args)` runs one CLI call; `schema` is the tool's output view
 * ({collectionPaths, collectionRefs}) or a function that loads it on first need.
 */
export function walk({ tool, input, run, schema, maxPages }) {
  const violations = [];
  const flag = (code, page, message, extra = {}) => violations.push({ code, page, ...extra, message });
  let route;
  const collection = pointer => {
    route ??= collectionRoute(typeof schema === 'function' ? schema() : schema);
    return route(pointer);
  };
  const slices = new Map();
  const open = new Map();
  const queries = Array.isArray(input?.queries) ? input.queries.length : undefined;
  const seenInputs = new Set();
  const seenPages = new Map();
  let call = { tool, input };
  let pages = 0;
  let replyChars = 0;

  function add(row, page) {
    const index = Number.isSafeInteger(row.index) ? row.index : queries === 1 || queries === undefined ? 0 : undefined;
    if (index === undefined) return flag('missingIndex', page, 'a row carries no index in a batch of several queries');
    const marker = row.rowPart;
    const saved = open.get(index);
    const prior = saved && saved.part < saved.of ? saved : undefined;
    const list = slices.get(index) ?? [];
    slices.set(index, list);
    const start = () => {
      const slice = structuredClone(row);
      delete slice.rowPart;
      list.push({ row: slice, parts: 1, pages: [page] });
      return list.at(-1);
    };
    if (!marker) {
      if (prior) flag('partOrder', page, `a whole row arrived while part ${prior.part} of ${prior.of} was open`, { index });
      open.delete(index);
      start();
      return undefined;
    }
    const { part, of } = marker;
    const counted = Number.isSafeInteger(part) && Number.isSafeInteger(of) && part >= 1 && part <= of;
    const follows = prior ? part === prior.part + 1 && of === prior.of : part === 1;
    if (!counted || !follows) {
      const want = prior ? `part ${prior.part + 1} of ${prior.of}` : 'part 1';
      flag('partOrder', page, `got part ${part} of ${of}, want ${want}`, { index });
    }
    if (!prior || !counted || !follows) {
      open.set(index, { slice: start(), part, of });
      return undefined;
    }
    try {
      if (marker.text !== undefined && !atPointer(prior.slice.row, row, marker.text).every(value => typeof value === 'string')) {
        throw new Violation('badContinuation', `rowPart.text ${marker.text} names no string to continue`);
      }
      if (marker.continues !== undefined) {
        const { array, index: at } = marker.continues;
        const [before, after] = atPointer(prior.slice.row, row, array);
        if (!Array.isArray(before) || !Array.isArray(after) || !after.length || !collection(array) || !Number.isSafeInteger(at) || at < 0 || at >= before.length) {
          throw new Violation('badContinuation', `rowPart.continues names a missing collection element ${array}/${at}`);
        }
      }
      prior.slice.row = mergePart(prior.slice.row, row, { ...marker, collection }, '');
    } catch (error) {
      if (!(error instanceof Violation)) throw error;
      flag(error.code, page, error.message, { index });
    }
    prior.slice.parts += 1;
    if (prior.slice.pages.at(-1) !== page) prior.slice.pages.push(page);
    open.set(index, { slice: prior.slice, part, of });
    return undefined;
  }

  while (call) {
    if (maxPages !== undefined && pages >= maxPages) {
      flag('notTerminated', pages, `stopped at --max-pages ${maxPages}; pending: ${call.tool} ${JSON.stringify(call.input)}`);
      break;
    }
    const key = JSON.stringify(call);
    if (seenInputs.has(key)) {
      flag('repeatedCursor', pages, `${call.tool} ${JSON.stringify(call.input)} was already sent`);
      break;
    }
    seenInputs.add(key);
    const ran = run([call.tool, JSON.stringify(call.input), '--json']);
    pages += 1;
    replyChars += ran.stdout.length;
    let reply;
    try {
      reply = JSON.parse(ran.stdout);
    } catch {
      flag('badPage', pages, `exit ${ran.status}: stdout is not one JSON reply: ${(ran.stdout || ran.stderr).trim().split('\n')[0] ?? ''}`);
      break;
    }
    if (!Array.isArray(reply?.results)) {
      if (pages === 1 && reply?.errorClass === 'correctInput') throw new WalkError('invalidInput', `${tool} refused the request: ${reply.error}`, 2);
      flag('pageError', pages, `${reply?.errorCode ?? 'error'}: ${reply?.error ?? JSON.stringify(reply)}`);
      break;
    }
    const digest = createHash('sha256').update(JSON.stringify(reply.results)).digest('hex');
    if (seenPages.has(digest)) {
      flag('repeatedPage', pages, `results equal page ${seenPages.get(digest)}`);
      break;
    }
    seenPages.set(digest, pages);
    if (!reply.results.length && reply.next) flag('noProgress', pages, 'a reply with a next carries no result row');
    for (const row of reply.results) {
      add(isRecord(reply.shared?.data) && isRecord(row.data) ? { ...row, data: withShared(reply.shared.data, row.data) } : row, pages);
    }
    if (reply.next == null) call = null;
    else if (!isRecord(reply.next) || reply.next.input === undefined) {
      flag('badContinuation', pages, `top-level next has no input: ${JSON.stringify(reply.next)}`);
      call = null;
    } else call = { tool: reply.next.tool ?? call.tool, input: reply.next.input };
  }

  for (const [index, { part, of }] of open) if (part < of) flag('unfinishedRow', pages, `the walk ended after part ${part} of ${of}`, { index });

  const rows = [];
  const partial = [];
  const rowErrors = [];
  for (const [index, list] of [...slices].toSorted(([a], [b]) => a - b)) {
    if (queries !== undefined && index >= queries) flag('unexpectedIndex', list[0].pages[0], `row index ${index} for ${queries} queries`, { index });
    const collections = {};
    list.forEach(({ row, pages: on }, slice) => {
      eachArray(row, '', (items, at) => {
        if (!collection(at)) return;
        const pattern = patternOf(at);
        collections[pattern] = (collections[pattern] ?? 0) + items.length;
        const seen = new Map();
        items.forEach((item, i) => {
          const text = JSON.stringify(item);
          if (seen.has(text)) flag('repeatedEvidence', on.at(-1), `${at}/${i} repeats ${at}/${seen.get(text)}: ${text}`, { index });
          else seen.set(text, i);
        });
      });
      const data = row.data;
      if (partialOf(data)) partial.push({ index, slice, isPartial: data.isPartial, partialReasons: data.partialReasons, terminalLimit: data.terminalLimit });
      if (isRecord(data) && (data.error !== undefined || data.errorCode !== undefined)) rowErrors.push({ index, slice, errorCode: data.errorCode, error: data.error });
    });
    checkWindows(list, index, flag);
    rows.push({ index, slices: list.length, parts: list.reduce((sum, item) => sum + item.parts, 0), collections });
  }
  if (queries !== undefined) {
    for (let index = 0; index < queries; index += 1) if (!slices.has(index)) flag('missingIndex', pages, `query ${index} has no row`, { index });
  }
  return {
    ok: violations.length === 0,
    tool,
    pages,
    replyChars,
    rows,
    partial,
    rowErrors,
    violations,
    results: [...slices].toSorted(([a], [b]) => a - b).flatMap(([, list]) => list.map(item => item.row)),
  };
}

/** A row's text-window slices (data.pagination with offset and length) must tile [0, total) in order. */
function checkWindows(list, index, flag) {
  const windows = list.map(({ row, pages }) => ({ pagination: row.data?.pagination, page: pages.at(-1), partial: partialOf(row.data) }));
  const totals = new Set(windows.map(({ pagination }) => pagination?.total).filter(Number.isSafeInteger));
  if (totals.size > 1) flag('totalChanged', windows.at(-1).page, `pagination.total took ${[...totals].join(', ')}`, { index });
  const text = windows.filter(({ pagination }) => typeof pagination?.unit === 'string' && Number.isSafeInteger(pagination.offset) && Number.isSafeInteger(pagination.length));
  for (let i = 1; i < text.length; i += 1) {
    const before = text[i - 1].pagination;
    const now = text[i].pagination;
    const end = before.offset + before.length;
    if (now.offset > end) flag('paginationGap', text[i].page, `${now.unit} ${end}–${now.offset} are on no page`, { index });
    if (now.offset < end) flag('paginationOverlap', text[i].page, `${now.unit} ${now.offset}–${end} arrive twice`, { index });
  }
  const last = text.at(-1);
  if (last && totals.size === 1 && !last.partial) {
    const [total] = totals;
    const end = last.pagination.offset + last.pagination.length;
    if (end < total) flag('paginationShort', last.page, `the last page ends at ${end} of ${total} ${last.pagination.unit} without a partial disclosure`, { index });
  }
}

function parse(argv) {
  const positional = [];
  const options = {};
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (['--bin', '--max-pages', '--out'].includes(arg)) {
      if (argv[i + 1] === undefined) throw new WalkError('invalidInput', `${arg} needs a value`, 2);
      options[arg.slice(2)] = argv[++i];
    } else if (arg.startsWith('--')) throw new WalkError('invalidInput', `unknown option ${arg} (see --help)`, 2);
    else positional.push(arg);
  }
  if (positional.length !== 2) throw new WalkError('invalidInput', 'pass <tool> and one JSON request', 2);
  const [tool, text] = positional;
  let input;
  try {
    input = JSON.parse(text);
  } catch (error) {
    throw new WalkError('invalidInput', `the request is not JSON: ${error.message}`, 2);
  }
  const maxPages = options['max-pages'] === undefined ? undefined : Number(options['max-pages']);
  if (maxPages !== undefined && !(Number.isSafeInteger(maxPages) && maxPages > 0)) throw new WalkError('invalidInput', '--max-pages takes a positive integer', 2);
  return { tool, input, maxPages, bin: options.bin && path.resolve(options.bin), out: options.out && path.resolve(options.out) };
}

function main(argv) {
  if (argv.includes('--help') || argv.includes('-h')) {
    console.log(HELP);
    return 0;
  }
  try {
    const { tool, input, maxPages, bin, out } = parse(argv);
    const run = cliRunner({ bin });
    const schema = () => {
      const ran = run(['schema', tool, '--view', 'output', '--json']);
      let view;
      try {
        view = JSON.parse(ran.stdout);
      } catch {
        throw new WalkError('invalidInput', `schema ${tool} --view output failed (exit ${ran.status}): ${ran.stderr.trim()}`, 2);
      }
      if (!Array.isArray(view.collectionPaths)) throw new WalkError('invalidInput', `schema ${tool} --view output has no collectionPaths`, 2);
      return view;
    };
    const { results, ...summary } = walk({ tool, input, run, schema, maxPages });
    if (out) fs.writeFileSync(out, `${JSON.stringify({ tool, input, results }, null, 1)}\n`);
    console.log(JSON.stringify(summary, null, 2));
    return summary.ok ? 0 : 1;
  } catch (error) {
    if (!(error instanceof WalkError)) throw error;
    console.log(JSON.stringify({ ok: false, error: { code: error.code, message: error.message } }));
    return error.exitCode;
  }
}

// exitCode, not exit(): a piped stdout flushes before the process ends.
if (isMainModule(import.meta.url)) process.exitCode = main(process.argv.slice(2));
