// Source maps (v3) for code that runs bundled: decode a map's VLQ mappings, find the map a bundle declares, map a
// generated position back to its source line and name, and, with no map, name the function around a position by
// scanning the generated text. `octocode-graph locate` and profile ingest (profile-ingest.ts) share it.
// Lines are 1-based and columns 1-based (as stack traces print them); maps store both 0-based.
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { dirname, isAbsolute, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import type { QueryData } from './query-index.ts';

const BASE64 =
  'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';
const DIGIT = new Int8Array(128).fill(-1);
for (let i = 0; i < BASE64.length; i++) DIGIT[BASE64.charCodeAt(i)] = i;

// Values grow past 2^31 only in corrupt maps; arithmetic instead of bit shifts keeps them exact anyway.
function vlqValue(raw: number) {
  const magnitude = Math.floor(raw / 2);
  return raw % 2 ? -magnitude : magnitude;
}

/** The signed values of a run of Base64 VLQ digits. */
export function decodeVlq(text: string): number[] {
  const values: number[] = [];
  let raw = 0,
    scale = 1;
  for (let i = 0; i < text.length; i++) {
    const d = DIGIT[text.charCodeAt(i)] ?? -1;
    if (d < 0) throw new Error(`invalid VLQ digit "${text[i]}"`);
    raw += (d & 31) * scale;
    if (d & 32) scale *= 32;
    else {
      values.push(vlqValue(raw));
      raw = 0;
      scale = 1;
    }
  }
  if (scale !== 1) throw new Error('VLQ value ends mid-number');
  return values;
}

export interface SourceMap {
  /** Each source as an absolute path, or as written when it is a non-file URL (webpack://…). */
  sources: (string | null)[];
  names: string[];
  /** Five numbers per segment: generated column, source, line, column (0-based) and name; -1 when absent. */
  segments: Int32Array;
  /** The first segment of each generated line; one more entry than lines. */
  lineStart: Int32Array;
}

// Mappings → flat segments, sorted by generated column within each line (the lookup's binary search needs it).
function decodeMappings(mappings: string) {
  const out: number[] = [];
  const starts = [0];
  const fields = [0, 0, 0, 0, 0];
  let k = 0,
    raw = 0,
    scale = 1;
  let col = 0,
    src = 0,
    line = 0,
    srcCol = 0,
    name = 0;
  const flush = () => {
    if (k === 0) return;
    if (k !== 1 && k !== 4 && k !== 5)
      throw new Error(`invalid source map segment of ${k} fields`);
    col += fields[0];
    if (k >= 4) {
      src += fields[1];
      line += fields[2];
      srcCol += fields[3];
    }
    if (k === 5) name += fields[4];
    out.push(
      col,
      k >= 4 ? src : -1,
      k >= 4 ? line : -1,
      k >= 4 ? srcCol : -1,
      k === 5 ? name : -1
    );
    k = 0;
  };
  const endLine = () => {
    const from = starts.at(-1)!;
    const to = out.length / 5;
    for (let s = from + 1; s < to; s++)
      if (out[s * 5] < out[(s - 1) * 5]) {
        const rows = [];
        for (let r = from; r < to; r++) rows.push(out.slice(r * 5, r * 5 + 5));
        rows.sort((a, b) => a[0] - b[0]);
        out.splice(from * 5, rows.length * 5, ...rows.flat());
        break;
      }
    starts.push(to);
  };
  for (let i = 0; i < mappings.length; i++) {
    const c = mappings.charCodeAt(i);
    if (c === 44 || c === 59) {
      if (scale !== 1) throw new Error('VLQ value ends mid-number');
      flush();
      if (c === 59) {
        endLine();
        col = 0;
      }
      continue;
    }
    const d = DIGIT[c] ?? -1;
    if (d < 0) throw new Error(`invalid VLQ digit "${mappings[i]}"`);
    raw += (d & 31) * scale;
    if (d & 32) {
      scale *= 32;
      continue;
    }
    if (k === 5) throw new Error('invalid source map segment of 6 fields');
    fields[k++] = vlqValue(raw);
    raw = 0;
    scale = 1;
  }
  if (scale !== 1) throw new Error('VLQ value ends mid-number');
  flush();
  endLine();
  return { segments: Int32Array.from(out), lineStart: Int32Array.from(starts) };
}

// A source as the map names it: a file URL or path (resolved from the map's directory and sourceRoot), else as written.
function sourceOf(source: unknown, root: string, mapDir: string) {
  if (typeof source !== 'string') return null;
  if (source.startsWith('file:')) return fileURLToPath(source);
  if (/^[a-z][\w+.-]*:/i.test(source) && !/^[a-z]:[\\/]/i.test(source))
    return source;
  if (isAbsolute(source)) return resolve(source);
  if (root.startsWith('file:')) return resolve(fileURLToPath(root), source);
  if (/^[a-z][\w+.-]*:/i.test(root) && !/^[a-z]:[\\/]/i.test(root))
    return `${root.replace(/\/?$/, '/')}${source}`;
  return resolve(mapDir, root, source);
}

/** A parsed map whose sources resolve from `mapDir`, the directory of the map file (of the bundle, for an inline map). */
export function parseSourceMap(json: unknown, mapDir: string): SourceMap {
  if (json === null || typeof json !== 'object' || Array.isArray(json))
    throw new Error('a source map is a JSON object');
  const map = json as Record<string, unknown>;
  if ('sections' in map)
    throw new Error('index source maps (sections) are not supported');
  if (map.version !== 3)
    throw new Error(`source map version ${String(map.version)} is not 3`);
  if (typeof map.mappings !== 'string' || !Array.isArray(map.sources))
    throw new Error('a source map needs mappings and sources');
  const root = typeof map.sourceRoot === 'string' ? map.sourceRoot : '';
  return {
    sources: map.sources.map(s => sourceOf(s, root, mapDir)),
    names: Array.isArray(map.names) ? map.names.map(String) : [],
    ...decodeMappings(map.mappings),
  };
}

export interface OriginalPosition {
  source: string | null;
  line: number;
  column: number;
  name: string | null;
}

/** The source position of a generated one: the last segment at or before the column; null when that maps nothing. */
export function originalPosition(
  map: SourceMap,
  line: number,
  column: number
): OriginalPosition | null {
  const l = line - 1;
  if (l < 0 || l >= map.lineStart.length - 1) return null;
  let lo = map.lineStart[l],
    hi = map.lineStart[l + 1] - 1,
    found = -1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    if (map.segments[mid * 5] <= column - 1) {
      found = mid;
      lo = mid + 1;
    } else hi = mid - 1;
  }
  if (found < 0 || map.segments[found * 5 + 1] < 0) return null;
  const s = found * 5;
  const name = map.segments[s + 4];
  return {
    source: map.sources[map.segments[s + 1]] ?? null,
    line: map.segments[s + 2] + 1,
    column: map.segments[s + 3] + 1,
    name: name < 0 ? null : (map.names[name] ?? null),
  };
}

export type FoundMap =
  | { ok: true; map: SourceMap; mapFile: string; sha256: string }
  | { ok: false; reason: string };

const digest = (bytes: string | Buffer) =>
  createHash('sha256').update(bytes).digest('hex');

// The last `//# sourceMappingURL=` (or `//@`, or `/*# … */`) in the text: the one a runtime follows.
function mappingUrl(text: string) {
  let url: string | null = null;
  for (const m of text.matchAll(
    /(?:\/\/|\/\*)[#@][ \t]*sourceMappingURL=([^\s*]+)/g
  ))
    url = m[1];
  return url;
}

function mapFrom(json: string, mapFile: string, mapDir: string): FoundMap {
  try {
    return {
      ok: true,
      map: parseSourceMap(JSON.parse(json), mapDir),
      mapFile,
      sha256: digest(json),
    };
  } catch (error) {
    return {
      ok: false,
      reason: `${mapFile} is not a usable source map: ${error instanceof Error ? error.message : String(error)}`,
    };
  }
}

/** The map a bundle declares: its sourceMappingURL (a file or a data: URL), else `<bundle>.map`; why not, when none. */
export function readSourceMap(bundle: string, text: string): FoundMap {
  const url = mappingUrl(text);
  if (url?.startsWith('data:')) {
    const comma = url.indexOf(',');
    const head = url.slice(5, comma);
    const body = url.slice(comma + 1);
    const json = head.split(';').includes('base64')
      ? Buffer.from(body, 'base64').toString('utf8')
      : decodeURIComponent(body);
    return mapFrom(json, 'inline', dirname(bundle));
  }
  const file = url
    ? url.startsWith('file:')
      ? fileURLToPath(url)
      : resolve(dirname(bundle), decodeURI(url))
    : `${bundle}.map`;
  let json: string;
  try {
    json = readFileSync(file, 'utf8');
  } catch (error) {
    const code = (error as NodeJS.ErrnoException).code ?? String(error);
    return {
      ok: false,
      reason: url
        ? `its sourceMappingURL names ${file}, which cannot be read (${code})`
        : `no sourceMappingURL comment and no ${file}`,
    };
  }
  return mapFrom(json, file, dirname(file));
}

/** A function in generated code: its name (null when it has none of its own), header start and closing brace. */
export interface FunctionRange {
  name: string | null;
  start: number;
  end: number;
}

// Words that end a statement head before a block (`if (x) {`), so the block is no function body.
const CONTROL = new Set([
  'if',
  'for',
  'while',
  'switch',
  'catch',
  'with',
  'function',
]);
// After these words a `/` starts a regular expression; after any other word or number it divides.
const BEFORE_REGEX = new Set([
  'return',
  'typeof',
  'instanceof',
  'in',
  'of',
  'new',
  'delete',
  'void',
  'throw',
  'case',
  'do',
  'else',
  'yield',
  'await',
]);
// Token placeholders for literals: none can be a name.
const STRING = '"',
  TEMPLATE = '`',
  REGEX = '/re';
const WORD = /[\w$#\u0080-￿]/;
const isName = (t: string | undefined) =>
  t !== undefined && /^[A-Za-z_$#\u0080-￿]/.test(t);
const OPERATOR = /[=<>!+\-*%&|^~?]/;

/**
 * Every function body in JavaScript text: declarations, expressions, methods and block-bodied arrows, each named by
 * its declaration, method key, or the variable or property it is assigned to. A lexer, not a parser: it skips
 * strings, templates, comments and regular expressions so their braces never count; expression-bodied arrows have no
 * block and are not listed.
 */
export function functionRanges(text: string): FunctionRange[] {
  const toks: string[] = [];
  const at: number[] = [];
  const opener = new Map<number, number>();
  const parens: number[] = [];
  const braces: (FunctionRange | null | typeof TEMPLATE)[] = [];
  const ranges: FunctionRange[] = [];
  const n = text.length;
  let i = 0;
  const push = (t: string, s: number) => {
    toks.push(t);
    at.push(s);
  };
  const assigned = (j: number) => {
    const t = toks[j];
    if (t === '=' && isName(toks[j - 1])) return toks[j - 1];
    if (
      t === ':' &&
      isName(toks[j - 1]) &&
      (toks[j - 2] === '{' || toks[j - 2] === ',')
    )
      return toks[j - 1];
    return null;
  };
  // The function whose body the `{` at token k opens, if it opens one.
  const bodyAt = (k: number): FunctionRange | null => {
    const prev = toks[k - 1];
    if (prev === '=>') {
      let head = k - 2;
      if (toks[head] === ')') head = opener.get(head) ?? head;
      if (toks[head - 1] === 'async') head--;
      return { name: assigned(head - 1), start: at[head], end: n };
    }
    if (prev !== ')') return null;
    const o = opener.get(k - 1);
    if (o === undefined) return null;
    let j = o - 1;
    const word = toks[j];
    const name = isName(word) && !CONTROL.has(word) ? word : null;
    if (name) j--;
    if (toks[j] === '*') j--;
    if (toks[j] === 'function') {
      const s = toks[j - 1] === 'async' ? j - 1 : j;
      return { name: name ?? assigned(s - 1), start: at[s], end: n };
    }
    if (name) return { name, start: at[o - 1], end: n };
    if (toks[o - 1] === ']') return { name: null, start: at[o], end: n };
    return null;
  };
  const regexAllowed = () => {
    const t = toks.at(-1);
    if (t === undefined) return true;
    if (WORD.test(t[0])) return BEFORE_REGEX.has(t);
    return ![')', ']', '}', STRING, TEMPLATE, REGEX].includes(t);
  };
  // Template text up to its end or its next `${`, which opens an expression the matching `}` returns from.
  const template = () => {
    while (i < n) {
      const c = text[i];
      if (c === '\\') i += 2;
      else if (c === '`') {
        i++;
        return;
      } else if (c === '$' && text[i + 1] === '{') {
        braces.push(TEMPLATE);
        i += 2;
        return;
      } else i++;
    }
  };
  while (i < n) {
    const c = text[i];
    if (c === ' ' || c === '\n' || c === '\t' || c === '\r') i++;
    else if (c === '/' && text[i + 1] === '/') {
      const end = text.indexOf('\n', i);
      i = end < 0 ? n : end;
    } else if (c === '/' && text[i + 1] === '*') {
      const end = text.indexOf('*/', i + 2);
      i = end < 0 ? n : end + 2;
    } else if (c === '"' || c === "'") {
      const s = i++;
      while (i < n && text[i] !== c && text[i] !== '\n')
        i += text[i] === '\\' ? 2 : 1;
      i++;
      push(STRING, s);
    } else if (c === '`') {
      push(TEMPLATE, i++);
      template();
    } else if (c === '/' && regexAllowed()) {
      const s = i++;
      let inClass = false;
      while (i < n && text[i] !== '\n' && (inClass || text[i] !== '/')) {
        if (text[i] === '\\') i++;
        else if (text[i] === '[') inClass = true;
        else if (text[i] === ']') inClass = false;
        i++;
      }
      if (text[i] === '/') {
        i++;
        while (i < n && /[a-z]/i.test(text[i])) i++;
        push(REGEX, s);
      } else {
        i = s + 1;
        push('/', s);
      }
    } else if (WORD.test(c)) {
      const s = i;
      while (i < n && WORD.test(text[i])) i++;
      push(text.slice(s, i), s);
    } else if (c === '(') {
      parens.push(toks.length);
      push('(', i++);
    } else if (c === ')') {
      const o = parens.pop();
      if (o !== undefined) opener.set(toks.length, o);
      push(')', i++);
    } else if (c === '{') {
      braces.push(bodyAt(toks.length));
      push('{', i++);
    } else if (c === '}') {
      const open = braces.pop();
      if (open === TEMPLATE) {
        i++;
        template();
        continue;
      }
      if (open) ranges.push({ ...open, end: i });
      push('}', i++);
    } else if (c === '=' && text[i + 1] === '>') {
      push('=>', i);
      i += 2;
    } else if (OPERATOR.test(c)) {
      const s = i;
      while (i < n && OPERATOR.test(text[i]) && text[i] !== '/') i++;
      push(text.slice(s, i), s);
    } else push(c, i++);
  }
  // A body still open where the text ends (cut text) runs to the end.
  for (const open of braces) if (open && open !== TEMPLATE) ranges.push(open);
  return ranges.sort((a, b) => a.start - b.start || b.end - a.end);
}

export interface EnclosingFunction extends FunctionRange {
  name: string;
  /** Functions without a name of their own between the position and this one. */
  anonymousInside: number;
}

/** The innermost named function around an offset; null when no named function holds it. */
export function enclosingFunction(
  ranges: readonly FunctionRange[],
  offset: number
): EnclosingFunction | null {
  const holding = ranges
    .filter(r => r.start <= offset && offset <= r.end)
    .sort((a, b) => b.start - a.start || a.end - b.end);
  for (let k = 0; k < holding.length; k++) {
    const { name } = holding[k];
    if (name) return { ...holding[k], name, anonymousInside: k };
  }
  return null;
}

/** The offset of a 1-based line and column (UTF-16 units, as maps and V8 count them); null past the text. */
export function offsetAt(text: string, line: number, column: number) {
  let offset = 0;
  for (let l = 1; l < line; l++) {
    const next = text.indexOf('\n', offset);
    if (next < 0) return null;
    offset = next + 1;
  }
  const end = text.indexOf('\n', offset);
  const length = (end < 0 ? text.length : end) - offset;
  return column >= 1 && column - 1 <= length ? offset + column - 1 : null;
}

export interface SymbolLike {
  id: string;
  file?: string;
  name?: string;
  kind?: string;
  line?: number;
  endLine?: number;
}

/** The innermost graph symbol of `file` whose declaration lines hold `line` (latest start, then shortest). */
export function enclosingSymbol(
  symbols: readonly SymbolLike[],
  file: string,
  line: number
): SymbolLike | null {
  let best: (SymbolLike & { line: number }) | null = null;
  for (const s of symbols) {
    const start = s.line;
    if ((s.file ?? s.id.split('#')[0]) !== file || start == null) continue;
    if (start > line || (s.endLine ?? start) < line) continue;
    if (
      !best ||
      start > best.line ||
      (start === best.line &&
        (s.endLine ?? start) < (best.endLine ?? best.line))
    )
      best = { ...s, line: start };
  }
  return best;
}

/** A position argument: `<file>:<line>:<column>`, the file a path or file:// URL. */
export function parsePosition(arg: string) {
  const m = /^(.*):(\d+):(\d+)$/.exec(arg);
  if (!m || !m[1] || Number(m[2]) < 1 || Number(m[3]) < 1) return null;
  const file = m[1].startsWith('file:') ? fileURLToPath(m[1]) : resolve(m[1]);
  return { file, line: Number(m[2]), column: Number(m[3]) };
}

export interface LocatedSymbol {
  id: string;
  name: string;
  kind: string;
  line: number | null;
  endLine?: number;
  /** `position`: the symbol's lines hold the mapped position; `name`: the only graph symbol with the generated name. */
  resolvedBy: 'position' | 'name';
}

export interface Located {
  input: string;
  bundle: string;
  line: number;
  column: number;
  /** The map used: its path, `inline`, or null without one. */
  map: string | null;
  original: {
    source: string | null;
    /** Root-relative path of the source under the graph root; null outside it or without a graph. */
    path: string | null;
    inGraph: boolean;
    line: number;
    column: number;
    name: string | null;
  } | null;
  symbol: LocatedSymbol | null;
  /** Without a mapping: the named function around the generated position and the graph symbols of that name. */
  generated?: { name: string; anonymousInside?: number; candidates?: string[] };
  /** Why the position maps to no source, or why no symbol resolves. */
  reason?: string;
  error?: string;
}

const symbolRow = (
  s: SymbolLike,
  resolvedBy: LocatedSymbol['resolvedBy']
): LocatedSymbol => ({
  id: s.id,
  name: s.name ?? s.id.split('#').slice(1).join('#'),
  kind: s.kind ?? 'symbol',
  line: s.line ?? null,
  ...(s.endLine != null && { endLine: s.endLine }),
  resolvedBy,
});

/** The graph path of an absolute file: root-relative under the graph root, else null. */
export function graphPathOf(root: string, file: string) {
  const rel = relative(root, file);
  return !rel || rel.startsWith('..') || isAbsolute(rel)
    ? null
    : rel.split(sep).join('/');
}

/** Map each `<bundle>:<line>:<column>` to its source and, with a graph, the enclosing graph symbol. */
export function locatePositions(
  args: readonly string[],
  graph?: QueryData
): Located[] {
  const files = new Map<string, { text: string; map: FoundMap }>();
  const ranges = new Map<string, FunctionRange[]>();
  const root = graph?.section ?? '';
  const inGraph = new Set((graph?.files ?? []).map(f => f.id));
  const symbols = graph?.symbols ?? [];
  return args.map(input => {
    const at = parsePosition(input)!;
    const row: Located = {
      input,
      bundle: at.file,
      line: at.line,
      column: at.column,
      map: null,
      original: null,
      symbol: null,
    };
    let file = files.get(at.file);
    if (!file) {
      let text: string;
      try {
        text = readFileSync(at.file, 'utf8');
      } catch (error) {
        return {
          ...row,
          error: `cannot read ${at.file}: ${(error as NodeJS.ErrnoException).code ?? String(error)}`,
        };
      }
      file = { text, map: readSourceMap(at.file, text) };
      files.set(at.file, file);
    }
    const offset = offsetAt(file.text, at.line, at.column);
    if (offset === null) {
      const lines = file.text.split('\n').length;
      return {
        ...row,
        reason:
          at.line > lines
            ? `line ${at.line} is past the end of ${at.file} (${lines} lines)`
            : `column ${at.column} is past the end of line ${at.line}`,
      };
    }
    if (file.map.ok) {
      row.map = file.map.mapFile;
      const o = originalPosition(file.map.map, at.line, at.column);
      if (o) {
        const path = o.source && root ? graphPathOf(root, o.source) : null;
        const known = path !== null && inGraph.has(path);
        row.original = { ...o, path, inGraph: known };
        const s = known ? enclosingSymbol(symbols, path, o.line) : null;
        if (s) row.symbol = symbolRow(s, 'position');
        else if (graph)
          row.reason = known
            ? `no graph symbol of ${path} holds line ${o.line}`
            : `the source ${o.source ?? '(unnamed)'} is not a file of the graph`;
        return row;
      }
      row.reason = `${file.map.mapFile} maps nothing at line ${at.line} column ${at.column}`;
    } else row.reason = file.map.reason;
    // No mapping: the named function around the generated position, matched to graph symbols by that name only.
    let found = ranges.get(at.file);
    if (!found) ranges.set(at.file, (found = functionRanges(file.text)));
    const fn = enclosingFunction(found, offset);
    if (!fn) return row;
    const candidates = graph
      ? symbols
          .filter(s => (s.name ?? '').split('.').pop() === fn.name)
          .map(s => s.id)
          .sort()
      : undefined;
    row.generated = {
      name: fn.name,
      ...(fn.anonymousInside && { anonymousInside: fn.anonymousInside }),
      ...(candidates && { candidates }),
    };
    if (candidates?.length === 1)
      row.symbol = symbolRow(
        symbols.find(s => s.id === candidates[0])!,
        'name'
      );
    return row;
  });
}

/** One line per position: where it maps, the name there and the enclosing symbol, or why it maps nowhere. */
export function formatLocated(rows: readonly Located[], cwd: string) {
  const shown = (p: string) => graphPathOf(cwd, p) ?? p;
  return rows
    .map(r => {
      if (r.error) return `${r.input}  error: ${r.error}`;
      const where = r.original
        ? `${r.original.path ?? (r.original.source ? shown(r.original.source) : '(unnamed source)')}:${r.original.line}:${r.original.column}${r.original.name ? ` ${r.original.name}` : ''}`
        : 'unmapped';
      const symbol = r.symbol
        ? `  in ${r.symbol.id}${r.symbol.line != null ? ` (declared at line ${r.symbol.line})` : ''}${r.symbol.resolvedBy === 'name' ? ' by name only' : ''}`
        : '';
      const generated = r.generated
        ? `  generated function ${r.generated.name}${r.generated.anonymousInside ? ` (inside ${r.generated.anonymousInside} anonymous)` : ''}${
            r.generated.candidates && r.generated.candidates.length !== 1
              ? r.generated.candidates.length
                ? `; graph symbols named ${r.generated.name}: ${r.generated.candidates.join(', ')}`
                : `; no graph symbol is named ${r.generated.name}`
              : ''
          }`
        : '';
      return `${r.input} → ${where}${symbol}${generated}${r.reason ? `  (${r.reason})` : ''}`;
    })
    .join('\n');
}
