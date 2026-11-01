// Response views: expand the compact wire shapes of tool results into the
// row objects the suites assert on, and follow their continuations.
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { isDeepStrictEqual } from 'node:util';
import { ROOT } from './paths.mjs';
import { contract } from './contract.mjs';

/** Typed continuation and related calls, preserving their exact input. */
export function nextHints(value, pathLabel = '') {
  const calls = [];
  const visit = (node, at) => {
    if (!node || typeof node !== 'object') return;
    if (['continue', 'related'].includes(node.kind) && typeof node.tool === 'string' && node.input && typeof node.input === 'object') {
      calls.push({ ...node, path: at });
      return;
    }
    for (const [key, child] of Object.entries(node)) visit(child, `${at}.${key}`);
  };
  visit(value, pathLabel);
  return calls;
}

/** A ghGetHistoryItem patch view without its gutter (`N\t` on every diff line, a bare tab on `\ No newline` markers): the raw patch. */
export function rawPatch(view) {
  return (view ?? '').split(/(?<=\n)/).map(line => line.startsWith('@@') ? line : line.replace(/^\d*\t/, '')).join('');
}

/**
 * The first line of a patch view whose gutter number is wrong (null when every
 * line is right). A right view numbers each line on its sign's side (native `number_patch`,
 * both-side numbering): kept and added lines carry their new-file line number,
 * removed lines their old-file number (both counted from the `@@ -a,b +c,d @@`
 * header), and a `\ No newline` marker a bare tab. With `oldLines`/`newLines`
 * (the file at the parent / at the change), each numbered line must also equal
 * that side's file line.
 */
export function patchNumbersIssue(view, sides = {}) {
  const next = { old: null, new: null };
  for (const line of (view ?? '').split('\n')) {
    if (line === '') continue;
    const issue = patchLineIssue(line, next, sides);
    if (issue) return issue;
  }
  return next.new === null ? 'no hunk header' : null;
}

const sameLine = (lines, number, text) => !lines || (lines[number - 1] ?? '').replace(/\r$/, '') === text.replace(/\r$/, '');

/** One patch view line checked against (and advancing) the `next` old/new line numbers. */
function patchLineIssue(line, next, { oldLines, newLines }) {
  const header = line.match(/^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/);
  if (header) { next.old = +header[1]; next.new = +header[2]; return null; }
  const m = line.match(/^(\d*)\t([\s\S])([\s\S]*)$/);
  if (!m || next.new === null) return `unnumbered: ${line.slice(0, 80)}`;
  const [, gutter, sign, text] = m;
  if (sign === '\\') return gutter === '' ? null : `numbered marker: ${line.slice(0, 80)}`;
  if (sign === '-') {
    if (+gutter !== next.old || !sameLine(oldLines, next.old, text)) return `old ${next.old}: ${line.slice(0, 80)}`;
    next.old++;
    return null;
  }
  if (+gutter !== next.new || !sameLine(newLines, next.new, text)) return `new ${next.new}: ${line.slice(0, 80)}`;
  if (sign !== '+') next.old++;
  next.new++;
  return null;
}

/** Collect objects matching a predicate anywhere in a value. */
export function collect(value, predicate, out = []) {
  if (value && typeof value === 'object') {
    if (predicate(value)) out.push(value);
    for (const child of Object.values(value)) collect(child, predicate, out);
  }
  return out;
}

/**
 * Expand ghGetHistoryItem's compact changed-file inventory — rows
 * "M +3 -1 [!reason ]path[ <- full/old/path]" or {"dir/": [rows named in dir]} —
 * into {path, status, additions, deletions, patchUnavailable?, previousPath?}.
 * An `omitted` file (GitHub sent neither patch nor counts) prints its letter
 * only, "M !omitted path", and expands without counts; a row
 * missing its counts for any other reason does not parse.
 * Object rows (patch responses) pass through unchanged.
 */
const INVENTORY_STATUS = { A: 'added', D: 'removed', M: 'modified', R: 'renamed', C: 'copied', T: 'changed', U: 'unchanged' };
function inventoryRow(row, dir = '') {
  const m = /^(\S+)(?: \+(\d+) -(\d+))?(?: !(\w+))? (.+?)(?: <- (.+))?$/.exec(row);
  if (!m || ((m[2] === undefined) !== (m[4] === 'omitted'))) return { path: null, raw: row };
  const file = { path: dir + m[5], status: INVENTORY_STATUS[m[1]] ?? m[1] };
  if (m[2] !== undefined) Object.assign(file, { additions: +m[2], deletions: +m[3] });
  if (m[4]) file.patchUnavailable = m[4];
  if (m[6]) file.previousPath = m[6];
  return file;
}
export function inventoryRows(items = []) {
  return (items ?? []).flatMap(item => {
    if (typeof item === 'string') return [inventoryRow(item)];
    if (item && typeof item === 'object' && !('path' in item)) return Object.entries(item).flatMap(([dir, rows]) => rows.map(row => inventoryRow(row, dir)));
    return [item];
  });
}

/**
 * structureSearch rows (tree and files) in one object shape. Every row is a
 * {dir, files: [...]} group whose `dir` is workspace-relative (like a row
 * path; "." is the workspace root). Entries ("<name>[/][ (<fields>)]";
 * fields: size in bytes, "symlink", "totalLines=N", "modified=<ISO instant>"; "/" marks
 * a directory) expand in order to {path: dir/name, size?, type?, totalLines?,
 * modifiedMs?}, `path` resolving against the response `root`. Object rows
 * pass through.
 */
export function structureFiles(items = []) {
  return (items ?? []).flatMap(item => {
    if (!item || typeof item !== 'object' || typeof item.dir !== 'string' || !Array.isArray(item.files)) return [item];
    return item.files.map(text => structureEntry(item.dir, text));
  });
}

function structureEntry(dir, text) {
  const m = /^(.*) \(([^()]*)\)$/.exec(text);
  let name = m ? m[1] : text;
  const row = {};
  if (name.endsWith('/')) { name = name.slice(0, -1); row.type = 'directory'; }
  row.path = name === '.' ? dir : joinDir(dir, name);
  for (const field of m ? m[2].split(', ') : []) {
    if (field === 'symlink') row.type = 'symlink';
    else if (field.startsWith('totalLines=')) row.totalLines = Number(field.slice(11));
    else if (field.startsWith('modified=')) row.modifiedMs = Date.parse(field.slice(9));
    else row.size = Number(field);
  }
  return row;
}

const joinDir = (dir, name) => (dir === '.' || dir === '' ? name : `${dir}/${name}`);

/** A response path resolved against that response's `root`. */
export const rootPath = (entry, p) => path.resolve(ROOT, p);

export function sourcePath(entry, location, fallback) {
  const value = location?.path ?? rowData(entry)?.path ?? fallback;
  if (typeof value !== 'string') throw new Error('response location has no source path');
  return value.startsWith('file:') ? fileURLToPath(value) : path.resolve(ROOT, value);
}

/**
 * lspSearch location rows in one shape: flat `data.matches` location
 * objects, or the compact per-file `data.files[].matches` reference rows
 * {line, column, endLine?, value?} long reference lists default to,
 * expanded to {path, displayRange, content}.
 */
export function lspLocations(entry, index = 0) {
  const payload = rowData(entry, index);
  if (Array.isArray(payload?.matches)) return payload.matches;
  return (payload?.files ?? []).flatMap(file => (file.matches ?? []).filter(ref => ref && typeof ref.line === 'number' && !ref.sites).map(ref => ({
    path: file.path,
    displayRange: { startLine: ref.line, startCharacter: ref.column, endLine: ref.endLine ?? ref.line },
    content: ref.value ?? '',
  })));
}

/**
 * astSearch / lspSearch outline rows as harness declarations: declaration
 * objects {symbolName, kind, line, endLine?, docStartLine?, exported?, shared?,
 * members?}. `name` = symbolName, `character` = column. `extra` (e.g. a
 * directory outline's file path) is merged in.
 */
const outlineContract = contract.tools.find(tool => tool.name === 'astSearch');
const outlineContextFields = new Set(Object.values(outlineContract.pageContextFields).flat());
const outlineCollections = new Set(outlineContract.collectionPaths.map(pointer => pointer.split('/').at(-1)));
function outlineContext(row, continued) {
  // Later fragments keep ancestor coordinates and nested provenance objects;
  // the occurrence itself retains its scalar identity on its first part only.
  return continued && row.members?.length > 0 && row.occurrenceId === undefined
    && Object.entries(row).every(([key, value]) => key === 'members'
      || (!outlineCollections.has(key) && (outlineContextFields.has(key) || (value !== null && typeof value === 'object'))));
}

/** Repeated context may omit fields, but cannot add or change values or array entries. */
export function unchangedContext(value, original) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return isDeepStrictEqual(value, original);
  return original !== null && typeof original === 'object' && !Array.isArray(original)
    && Object.entries(value).every(([key, child]) => Object.hasOwn(original, key) && unchangedContext(child, original[key]));
}

export function outlineRows(rows = [], extra = {}, parent = {}, state = {}) {
  const { continued = false, context = new Map(), scope = 0 } = state;
  // Containers nest members under them (`members`, with fields every member
  // shares in the container's `shared`); flatten them in source order with
  // `parent`/`parentLine` restored.
  return (rows ?? []).flatMap(row => {
    if (!row || typeof row !== 'object' || Array.isArray(row)) throw new Error(`outline row is not a declaration object: ${JSON.stringify(row)}`);
    const { symbolName, column, members, shared, ...rest } = row;
    const decl = { ...extra, ...parent, ...rest, name: symbolName };
    if (column !== undefined) decl.character = column;
    const nested = (members ?? []).map(member => ({ ...shared, ...member }));
    const key = JSON.stringify([scope, extra.path, parent.parent, parent.parentLine, symbolName, row.kind, row.line]);
    const own = outlineContext(row, continued) && unchangedContext(decl, context.get(key)) ? [] : [decl];
    // Every declaration's first appearance is the context a later part's fragment of it repeats: a split nested
    // two levels deep (file > symbol > members) repeats the symbol, which carries no occurrenceId.
    if (!context.has(key)) context.set(key, decl);
    return [...own, ...outlineRows(nested, extra, { parent: symbolName, parentLine: row.line }, { continued, context, scope })];
  });
}

/** Every declaration of an astSearch symbols row (single file or directory). */
export function declarations(entry, index = 0, context = new Map()) {
  const data = rowData(entry, index);
  if (!data) return [];
  const part = entry.sc?.results?.[index]?.rowPart;
  const continued = part?.part > 1 && Boolean(part.continues);
  const scope = entry.sc?.results?.[index]?.index ?? index;
  const state = { continued, context, scope };
  return [...outlineRows(data.symbols, {}, {}, state), ...(data.files ?? []).flatMap(file => outlineRows(file.symbols, { path: file.path }, {}, state))];
}

/**
 * astSearch match rows of a response as {path, line, endLine?, value, metavarRanges? (captureText only)};
 * a non-object row is kept as {path, raw}.
 */
export function astMatchRows(entry, index = 0) {
  return (rowData(entry, index)?.files ?? []).flatMap(file => (file.matches ?? []).map(row => ({ path: file.path, ...(row && typeof row === 'object' ? row : { raw: row }) })));
}

/**
 * lspSearch callers as {name, kind, detail?, path, lines, declLine?, via?}:
 * flat `data.matches` call-hierarchy edges, or the compact per-file call
 * rows {symbolName, kind, line, endLine?, sites, detail?, via?} of a
 * `callers` data (a `callees` result lists no callers).
 */
export function lspCallers(entry, index = 0) {
  const payload = rowData(entry, index);
  if (Array.isArray(payload?.matches)) {
    return payload.matches.filter(item => item.from).map(item => ({
      name: item.from.symbolName, kind: item.from.kind, detail: item.from.detail, path: item.from.path,
      lines: (item.fromRanges ?? []).map(range => range.startLine), declLine: item.from.displayRange?.startLine,
    }));
  }
  if (payload?.kind === 'callees') return [];
  return (payload?.files ?? []).flatMap(file => (file.matches ?? []).filter(row => row && Array.isArray(row.sites)).map(row => ({
    name: row.symbolName, kind: row.kind, detail: row.detail, path: file.path,
    lines: row.sites.map(site => site.line), declLine: row.line,
    via: row.via ? { name: row.via.symbolName, line: row.via.line, ...(row.via.path ? { path: row.via.path } : {}) } : undefined,
  })));
}

const SOURCE_GAP = /^(?:--|\.\.\. \[(?:lines? \d+(?:-\d+)?|\d+ gaps in lines \d+-\d+) (?:omitted|not requested|not selected)\] \.\.\.)$/;

/**
 * A file-read row's source view. Reads number source lines in `content`
 * (`<line>\t<text>`, omission markers unnumbered; docs/TOOL_DATA_CONTRACT.md
 * "Numbered source content") and then omit `sourceLineRanges`: `text` strips
 * the gutter and `ranges` comes from the numbers; `--` separators stay in
 * `text` as their own lines. Verbatim views pass through.
 */
export function sourceView(file) {
  const content = file?.content ?? '';
  const records = content.split('\n');
  const trailing = records.at(-1) === '' ? records.pop() : undefined;
  // Non-adjacent windows are separated by one unnumbered `--` line (grep's
  // group separator, TOOL_DATA_CONTRACT "Numbered source content"); older
  // views used `... [lines N-M omitted] ...` markers.
  const numbered = !file?.sourceLineRanges && records.length > 0 && records.every(l => /^\d+\t/.test(l) || SOURCE_GAP.test(l));
  // Verbatim views keep `sourceLineRanges` as `{line, endLine}`; the
  // view's ranges are `{start, end}` in both branches.
  if (!numbered) return { text: content, ranges: (file?.sourceLineRanges ?? []).map(r => ({ start: r.line, end: r.endLine })), numbered: false };
  const ranges = [];
  const text = records.map(l => {
    const m = /^(\d+)\t/.exec(l);
    if (!m) return l;
    const n = Number(m[1]);
    const last = ranges.at(-1);
    if (last && last.end + 1 === n) last.end = n; else ranges.push({ start: n, end: n });
    return l.slice(m[0].length);
  });
  if (trailing !== undefined) text.push('');
  return { text: text.join('\n'), ranges, numbered: true };
}

/** Exact source [line, text] pairs from a gutter or sourceLineRanges; gaps are not evidence. */
export function sourceLines(file) {
  const view = sourceView(file);
  if (view.numbered) return file.content.split('\n').flatMap(record => {
    const match = /^(\d+)\t/.exec(record);
    return match ? [[Number(match[1]), record.slice(match[0].length)]] : [];
  });
  const records = view.text.split('\n');
  if (records.at(-1) === '') records.pop();
  return rangedLines(records, view.ranges);
}

/** `records` laid over `ranges` in order; one omission marker may precede a range or follow the last. */
function rangedLines(records, ranges) {
  let remaining = rangedLineCount(ranges);
  const pairs = [];
  let cursor = 0;
  let previousEnd;
  for (const { start, end } of ranges) {
    if (records.length - cursor > remaining && gapBefore(records[cursor], start, previousEnd)) cursor++;
    for (let line = start; line <= end; line++) {
      if (cursor >= records.length) throw new Error('Source content is shorter than its line ranges');
      pairs.push([line, records[cursor++]]);
      remaining--;
    }
    previousEnd = end;
  }
  if (cursor < records.length && gapAfter(records[cursor], previousEnd)) cursor++;
  if (cursor !== records.length) throw new Error('Source content does not match its line ranges');
  return pairs;
}

function rangedLineCount(ranges) {
  let count = 0;
  for (const { start, end } of ranges) {
    if (!Number.isSafeInteger(start) || !Number.isSafeInteger(end) || start < 1 || end < start)
      throw new Error('Invalid source line range');
    count += end - start + 1;
  }
  return count;
}

/**
 * Whether `record` is an extra omission marker before the range at `start`. Source
 * text resembling a marker remains evidence when its coordinates belong to the range.
 */
function gapBefore(record, start, previousEnd) {
  if (!SOURCE_GAP.test(record)) return false;
  const gap = /\blines? (\d+)(?:-(\d+))? /.exec(record);
  return Boolean(gap && Number(gap[2] ?? gap[1]) < start) || (record === '--' && previousEnd !== undefined && start > previousEnd + 1);
}

/** Whether `record` is an omission marker after the last range (ending at `previousEnd`). */
function gapAfter(record, previousEnd) {
  if (!SOURCE_GAP.test(record)) return false;
  const gap = /\blines? (\d+)(?:-(\d+))? /.exec(record);
  return Boolean(gap && Number(gap[1]) > previousEnd);
}

export function rowData(entry, index = 0) {
  const row = entry.sc?.results?.[index];
  return row?.data;
}

/** The first `next.*`/`hints.*` continuation whose path ends with `.<key>` (e.g. `continue`, `hints.read`). */
export function findHint(sc, key) {
  return nextHints(sc).find(h => h.path.endsWith(`.${key}`));
}

/**
 * Follow `next.<key>` from `first` with `client.follow` until it disappears or
 * `max` pages are collected; returns the page entries. A page with a call error
 * (or a row error, unless `rowErrors: false`) ends the walk; `keepError` keeps
 * that page as the last entry. `label` names each followed page "<label> page N".
 */
export async function walk(client, first, key, { max, keepError = false, rowErrors = true, label } = {}) {
  const pages = [first];
  let current = first;
  while (pages.length < max) {
    const h = findHint(current.sc, key);
    if (!h) break;
    current = await client.follow(h, ...(label === undefined ? [] : [`${label} page ${pages.length + 1}`]));
    if (current.isError || (rowErrors && current.rowErrors)) { if (keepError) pages.push(current); break; }
    pages.push(current);
  }
  return pages;
}
