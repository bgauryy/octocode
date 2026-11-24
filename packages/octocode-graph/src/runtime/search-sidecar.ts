// search-index.json beside a saved graph's data.json: what search needs from the source that data.json does not
// hold. Node only (the query engine itself never reads files): the CLI, MCP and the viewer server attach it with
// withSearchIndex when they load a graph, and the first search reads it, or builds and writes it when it is missing
// or was built from another data.json (its key is the sha256 of the data.json bytes).
//   docs          the comment block right above each declaration (`//`, `///`, `/** */`, `#`, `--`, past
//                 decorators and attributes), a Python docstring under its `def` or `class`, and each file's
//                 leading comment, read from the source by the declaration's line
//   symbols       the declarations the build's chunks hold (chunks/<id>/data.json.gz) that data.json's call-graph
//                 symbols leave out, so search finds a declaration nothing calls
// A file whose size differs from the one the graph recorded (edited since) is not read: it is listed in `unread`,
// and search warns about it. Each chunk's part (its declarations and doc comments) is cached in the shared graph cache
// (graph-cache.ts, namespace `search-index`) by the digest of the chunk result it was read from, so another result
// directory whose chunks came from that cache reads no source for them.
import { createHash } from 'node:crypto';
import { readFileSync, renameSync, statSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { gunzipSync } from 'node:zlib';
import {
  attachSearchIndex,
  SIDECAR_VERSION,
  type SearchSidecar,
  type SearchSymbol,
} from './search-index.ts';
import { CHUNK_DATA } from './section.ts';
import { getDerived, putDerived, type DerivedOptions } from './graph-cache.ts';
import type { QueryData } from './query-index.ts';
import type { FileNode } from './types.ts';
import { absOf, prefixOf, rootOf } from '../ids.ts';

/** The sidecar's file name, beside data.json. */
export const SEARCH_INDEX = 'search-index.json';

type Style = 'slash' | 'hash' | 'dash';
// Comment syntax by file extension; every other language comments with `//` and `/* */`.
const HASH =
  /\.(py|pyi|rb|sh|bash|zsh|pl|pm|r|ya?ml|toml|tf|tfvars|hcl|cmake|nix|exs?|jl|ps1|coffee|mk)$/i;
const DASH = /\.(lua|sql|hs|elm|ada|adb|ads|vhdl?)$/i;
const styleOf = (file: string): Style =>
  HASH.test(file) ? 'hash' : DASH.test(file) ? 'dash' : 'slash';
const isPython = (file: string) => /\.pyi?$/i.test(file);

// A line between a comment and its declaration that belongs to the declaration: a decorator or annotation
// (`@cache`, `@Override`), a Rust attribute (`#[derive(..)]`), a C# attribute (`[Obsolete]`).
const ATTRIBUTE = /^\s*(@[\w.]+|#!?\[|\[[A-Z][\w.]*(\(|\]))/;

function commentLine(line: string, style: Style) {
  const t = line.trim();
  if (style === 'hash')
    return t.startsWith('#') && !t.startsWith('#!') && !t.startsWith('#[')
      ? t.replace(/^#+/, '')
      : null;
  if (style === 'dash') return t.startsWith('--') ? t.replace(/^-+/, '') : null;
  return t.startsWith('//') ? t.replace(/^\/\/[/!]?/, '') : null;
}

const clean = (parts: string[]) => parts.join(' ').replace(/\s+/g, ' ').trim();

/** The comment block that ends right above line `at` (0-based), past attribute lines; '' when there is none. */
export function commentAbove(lines: string[], at: number, style: Style) {
  let i = at - 1;
  while (
    i >= 0 &&
    ATTRIBUTE.test(lines[i]) &&
    commentLine(lines[i], style) === null
  )
    i--;
  const parts: string[] = [];
  if (i >= 0 && style === 'slash' && lines[i].trim().endsWith('*/')) {
    // A block comment: up to the line that opens it.
    let j = i;
    while (j >= 0 && !lines[j].includes('/*')) j--;
    if (j < 0) return '';
    for (let k = j; k <= i; k++) {
      let text = lines[k];
      if (k === j)
        text = text.slice(text.indexOf('/*') + 2).replace(/^\*+!?/, '');
      const end = text.indexOf('*/');
      if (end >= 0) text = text.slice(0, end);
      parts.push(text.replace(/^\s*\*+/, ''));
    }
    return clean(parts);
  }
  for (; i >= 0; i--) {
    const text = commentLine(lines[i], style);
    if (text === null) break;
    parts.unshift(text);
  }
  return clean(parts);
}

/** The docstring of a Python `def` or `class` declared at line `at` (0-based): the string its body starts with. */
export function docstringBelow(lines: string[], at: number) {
  // The signature may span lines: it ends at the first line ending in `:` (a comment after it aside).
  let i = at;
  while (i < lines.length && !/:\s*(#.*)?$/.test(lines[i])) i++;
  i++;
  while (i < lines.length && !lines[i].trim()) i++;
  const first = lines[i]?.trim() ?? '';
  const open = /^[rRuUbBfF]{0,2}("""|'''|"|')/.exec(first);
  if (!open) return '';
  const quote = open[1];
  const body = first.slice(open[0].length);
  const close = body.indexOf(quote);
  if (close >= 0) return clean([body.slice(0, close)]);
  const parts = [body];
  for (let k = i + 1; k < lines.length; k++) {
    const end = lines[k].indexOf(quote);
    if (end >= 0) {
      parts.push(lines[k].slice(0, end));
      break;
    }
    parts.push(lines[k]);
  }
  return clean(parts);
}

/** A file's leading comment (after a shebang and blank lines), or a Python module docstring. */
export function leadingComment(lines: string[], file: string) {
  let i = 0;
  if (lines[0]?.startsWith('#!')) i++;
  while (i < lines.length && !lines[i].trim()) i++;
  if (isPython(file)) {
    const doc = docstringBelow(['x:', ...lines.slice(i)], 0);
    if (doc) return doc;
  }
  const style = styleOf(file);
  if (style === 'slash' && lines[i]?.trim().startsWith('/*')) {
    let j = i;
    while (j < lines.length && !lines[j].includes('*/')) j++;
    return commentAbove(lines, j + 1, style);
  }
  let j = i;
  while (j < lines.length && commentLine(lines[j], style) !== null) j++;
  return j > i ? commentAbove(lines, j, style) : '';
}

const digestOf = (bytes: Buffer) =>
  createHash('sha256').update(bytes).digest('hex');

/** The graph-cache namespace of a chunk's search part. */
export const SEARCH_NAMESPACE = 'search-index';

/** What one chunk adds: its declarations (root-relative ids), its files' doc comments, and the files not read. */
interface ChunkPart {
  declarations: SearchSymbol[];
  docs: Record<string, string>;
  unread: SearchSidecar['unread'];
}

const isChunkPart = (value: unknown): value is ChunkPart =>
  typeof value === 'object' &&
  value !== null &&
  Array.isArray((value as ChunkPart).declarations) &&
  typeof (value as ChunkPart).docs === 'object' &&
  Array.isArray((value as ChunkPart).unread);

// The doc comments of `files` (a file's leading comment, and each declaration's) read from the source under `root`.
function docsOf(
  root: string,
  files: readonly FileNode[],
  declarations: readonly { id: string; file: string; line?: number | null }[]
) {
  const byFile = new Map<string, { id: string; line: number }[]>();
  for (const s of declarations)
    if (s.line) {
      const list = byFile.get(s.file) ?? [];
      list.push({ id: s.id, line: s.line });
      byFile.set(s.file, list);
    }
  const docs: Record<string, string> = {};
  const unread: SearchSidecar['unread'] = [];
  for (const f of files) {
    if (f.kind === 'package' || !root) continue;
    let source: Buffer;
    try {
      source = readFileSync(absOf(root, f.id));
    } catch (error) {
      unread.push({
        file: f.id,
        reason: (error as NodeJS.ErrnoException).code ?? 'unreadable',
      });
      continue;
    }
    if (f.check?.bytes != null && source.length !== f.check.bytes) {
      unread.push({
        file: f.id,
        reason: 'changed since the graph was written',
      });
      continue;
    }
    const lines = source.toString('utf8').split(/\r?\n/);
    const style = styleOf(f.id);
    const head = leadingComment(lines, f.id);
    if (head) docs[f.id] = head;
    for (const s of byFile.get(f.id) ?? []) {
      const at = s.line - 1;
      const doc =
        (isPython(f.id) && /^\s*(async\s+def|def|class)\s/.test(lines[at] ?? '')
          ? docstringBelow(lines, at)
          : '') || commentAbove(lines, at, style);
      if (doc) docs[s.id] = doc;
    }
  }
  return { docs, unread };
}

// One chunk's part: from the cache by the digest of its result (chunks/<id>/data.json.gz), else read and cached.
function chunkPart(
  graph: QueryData,
  dir: string,
  chunk: { id: string; path: string },
  cache: DerivedOptions | false
): ChunkPart | null {
  let gz: Buffer;
  try {
    gz = readFileSync(join(dir, 'chunks', chunk.id, CHUNK_DATA));
  } catch {
    return null;
  }
  const root = rootOf(graph);
  const prefix = prefixOf(chunk.path);
  const parts = { version: SIDECAR_VERSION, chunk: digestOf(gz), prefix, root };
  if (cache) {
    const hit = getDerived(SEARCH_NAMESPACE, parts, cache);
    if (isChunkPart(hit)) return hit;
  }
  let section: { declarations?: (SearchSymbol & { file: string })[] };
  try {
    section = JSON.parse(gunzipSync(gz).toString()) as typeof section;
  } catch {
    return null;
  }
  const declarations = (section.declarations ?? []).map(d => ({
    id: prefix + d.id,
    file: prefix + d.file,
    name: d.name,
    kind: d.kind,
    line: d.line ?? null,
    endLine: d.endLine ?? null,
    exported: !!d.exported,
    ...(d.params != null && { params: d.params }),
    ...(d.returns != null && { returns: d.returns }),
  }));
  const files = (graph.files ?? []).filter(f => f.chunk === chunk.id);
  const part = { declarations, ...docsOf(root, files, declarations) };
  if (cache) putDerived(SEARCH_NAMESPACE, parts, part, cache);
  return part;
}

// The sidecar of `graph`, parsed from `bytes` of the data.json at `dataPath`: each chunk's part, and the files no
// chunk result covers (a graph written before chunk results) read here.
function sidecarFor(
  dataPath: string,
  graph: QueryData,
  bytes: Buffer,
  cache: DerivedOptions | false
): SearchSidecar {
  const dir = dirname(dataPath);
  const parts = (graph.chunks ?? []).flatMap(c => {
    const part = chunkPart(graph, dir, c, cache);
    return part ? [{ id: c.id, part }] : [];
  });
  const covered = new Set(parts.map(p => p.id));
  const rest = (graph.files ?? []).filter(
    f => !f.chunk || !covered.has(f.chunk)
  );
  const restFiles = new Set(rest.map(f => f.id));
  const loose = docsOf(
    rootOf(graph),
    rest,
    (graph.symbols ?? []).filter(s => restFiles.has(s.file))
  );
  const known = new Set((graph.symbols ?? []).map(s => s.id));
  const symbols: SearchSymbol[] = [];
  for (const { part } of parts)
    for (const d of part.declarations)
      if (!known.has(d.id)) {
        known.add(d.id);
        symbols.push(d);
      }
  return {
    version: SIDECAR_VERSION,
    data: digestOf(bytes),
    symbols,
    docs: Object.assign({}, ...parts.map(p => p.part.docs), loose.docs),
    unread: [...parts.flatMap(p => p.part.unread), ...loose.unread],
  };
}

/** How the sidecar is built: the graph already parsed from `bytes`, and the shared cache to use (false: none). */
export interface BuildOptions {
  data?: QueryData;
  bytes?: Buffer;
  cache?: DerivedOptions | false;
}

/**
 * Builds the search sidecar of the graph at `dataPath` (a data.json) and writes it beside it as search-index.json
 * (a result directory this process cannot write keeps it in memory only). Chunk parts come from the shared graph
 * cache when their chunk result is unchanged, and go into it otherwise.
 */
export function buildSearchIndex(
  dataPath: string,
  { bytes = readFileSync(dataPath), data, cache = {} }: BuildOptions = {}
): SearchSidecar {
  const sidecar = sidecarFor(
    dataPath,
    data ?? (JSON.parse(bytes.toString('utf8')) as QueryData),
    bytes,
    cache
  );
  const file = join(dirname(dataPath), SEARCH_INDEX);
  const temp = `${file}.${process.pid}.tmp`;
  try {
    writeFileSync(temp, JSON.stringify(sidecar));
    renameSync(temp, file);
  } catch {
    // A result directory this process cannot write: the sidecar serves this process only.
  }
  return sidecar;
}

/** The sidecar beside `dataPath` when it was built from exactly these data.json bytes, else null. */
export function readSearchIndex(
  dataPath: string,
  bytes: Buffer = readFileSync(dataPath)
): SearchSidecar | null {
  try {
    const found = JSON.parse(
      readFileSync(join(dirname(dataPath), SEARCH_INDEX), 'utf8')
    ) as SearchSidecar;
    return found.version === SIDECAR_VERSION && found.data === digestOf(bytes)
      ? found
      : null;
  } catch {
    return null;
  }
}

const statKey = (path: string) => {
  const s = statSync(path, { throwIfNoEntry: false });
  return s ? `${s.ino}:${s.size}:${s.mtimeMs}` : null;
};

/**
 * Attaches the search sidecar of the data.json `D` was parsed from at `dataPath`: the first search reads it, or
 * builds it when it is missing or stale. Call it right after parsing; if data.json is replaced before that first
 * search, the sidecar is built for `D` in memory and not written (it would not describe the file on disk).
 */
export function withSearchIndex(
  D: QueryData,
  dataPath: string,
  { cache = {} }: Pick<BuildOptions, 'cache'> = {}
) {
  const loaded = statKey(dataPath);
  attachSearchIndex(D, () => {
    const bytes = readFileSync(dataPath);
    if (statKey(dataPath) !== loaded)
      return sidecarFor(dataPath, D, bytes, cache);
    return (
      readSearchIndex(dataPath, bytes) ??
      buildSearchIndex(dataPath, { data: D, bytes, cache })
    );
  });
  return D;
}
