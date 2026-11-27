// Which bytes of a source file are strings or comments, per extension: the battery oracle drops a ripgrep hit
// whose offset is not code. Interpolated parts of strings (JS/TS `${}`, Python f-string `{}`, C# `$"{}"`,
// Scala `s"${}"`) are code. A single-line string that meets a newline ends there, so one misread quote costs at
// most its line.

const FAMILIES = {
  c: ['c', 'h', 'cc', 'cpp', 'cxx', 'c++', 'hpp', 'hh', 'hxx', 'h++', 'cu', 'cuh', 'inl', 'ipp', 'tpp'],
  java: ['java'],
  csharp: ['cs'],
  go: ['go'],
  js: ['js', 'mjs', 'cjs', 'jsx', 'ts', 'mts', 'cts', 'tsx'],
  rust: ['rs'],
  scala: ['scala', 'sc'],
  python: ['py', 'pyi'],
};
const FAMILY_OF = new Map(Object.entries(FAMILIES).flatMap(([family, exts]) => exts.map((ext) => [ext, family])));

/** The lexer family of a file extension (no dot), or null when none is defined. */
export const familyOf = (ext) => FAMILY_OF.get(String(ext).toLowerCase()) ?? null;

const DQ = 0x22, SQ = 0x27, BT = 0x60, BS = 0x5c, NL = 0x0a, SL = 0x2f, STAR = 0x2a, HASH = 0x23;
const LB = 0x7b, RB = 0x7d, LSQ = 0x5b, RSQ = 0x5d, RP = 0x29, DOLLAR = 0x24, AT = 0x40, LP = 0x28;
const isIdent = (c) => (c >= 0x30 && c <= 0x39) || (c >= 0x41 && c <= 0x5a) || (c >= 0x61 && c <= 0x7a) || c === 0x5f || c >= 0x80;
const isSpace = (c) => c === 0x20 || c === 0x09 || c === NL || c === 0x0d;
const REGEX_KEYWORDS = new Set(['return', 'typeof', 'case', 'in', 'of', 'delete', 'void', 'throw', 'new', 'else', 'do', 'yield', 'await', 'instanceof']);
const PYTHON_PREFIXES = new Set(['', 'r', 'u', 'b', 'f', 'br', 'rb', 'fr', 'rf']);
const CPP_RAW = new Set(['R', 'u8R', 'uR', 'UR', 'LR']);
const RUST_RAW = new Set(['r', 'br', 'cr']);

/** UTF-8 length of the code point starting with lead byte c. */
const utf8Length = (c) => (c < 0x80 ? 1 : c >= 0xf0 ? 4 : c >= 0xe0 ? 3 : 2);

class Scanner {
  constructor(bytes, family) {
    this.b = bytes;
    this.f = family;
    this.mask = new Uint8Array(bytes.length);
  }

  mark(from, to) {
    this.mask.fill(1, from, Math.min(to, this.b.length));
  }

  /** The identifier ending right before index i, as text. */
  wordBefore(i) {
    let start = i;
    while (start > 0 && isIdent(this.b[start - 1])) start -= 1;
    return this.b.toString('latin1', start, i);
  }

  lineEnd(i) {
    const end = this.b.indexOf(NL, i);
    return end < 0 ? this.b.length : end;
  }

  /** Code from i; inside braces, returns the index of the `}` that closes them. */
  code(start, inBraces) {
    const { b, f } = this;
    let depth = 0;
    let regexOk = true;
    let i = start;
    while (i < b.length) {
      const c = b[i];
      if (inBraces && c === LB) depth += 1;
      if (inBraces && c === RB) {
        if (depth === 0) return i;
        depth -= 1;
      }
      if (f === 'python' ? c === HASH : c === SL && b[i + 1] === SL) {
        const end = this.lineEnd(i);
        this.mark(i, end);
        i = end;
        continue;
      }
      if (f !== 'python' && c === SL && b[i + 1] === STAR) {
        i = this.block(i);
        continue;
      }
      if (f === 'js' && c === SL && regexOk) {
        const end = this.regex(i);
        if (end > i) {
          this.mark(i, end);
          i = end;
          regexOk = false;
          continue;
        }
      }
      if (c === DQ || c === SQ || c === BT) {
        const end = this.literal(i);
        if (end > i) {
          i = end;
          regexOk = false;
          continue;
        }
      }
      if (isIdent(c) || c === DOLLAR) {
        let end = i + 1;
        while (end < b.length && (isIdent(b[end]) || b[end] === DOLLAR)) end += 1;
        regexOk = REGEX_KEYWORDS.has(b.toString('latin1', i, end));
        i = end;
        continue;
      }
      if (!isSpace(c)) regexOk = c !== RP && c !== RSQ && c !== RB;
      i += 1;
    }
    return b.length;
  }

  block(start) {
    const { b } = this;
    let depth = 0;
    let i = start;
    while (i < b.length) {
      if (b[i] === SL && b[i + 1] === STAR) {
        depth += 1;
        i += 2;
        if (!(this.f === 'rust' || this.f === 'scala')) depth = 1;
        continue;
      }
      if (b[i] === STAR && b[i + 1] === SL) {
        depth -= 1;
        i += 2;
        if (depth === 0) break;
        continue;
      }
      i += 1;
    }
    this.mark(start, i);
    return i;
  }

  /** A JS regex literal from the `/` at start: its end after the flags, or start when it is not one. */
  regex(start) {
    const { b } = this;
    let inClass = false;
    for (let i = start + 1; i < b.length; i += 1) {
      const c = b[i];
      if (c === NL || (i === start + 1 && (c === SL || c === STAR))) return start;
      if (c === BS) i += 1;
      else if (c === LSQ) inClass = true;
      else if (c === RSQ) inClass = false;
      else if (c === SL && !inClass) {
        let end = i + 1;
        while (end < b.length && isIdent(b[end])) end += 1;
        return end;
      }
    }
    return start;
  }

  /** A string or character literal at start: the index after it, or start when the quote opens none. */
  literal(start) {
    const { b, f } = this;
    const q = b[start];
    if (q === BT) {
      if (f === 'js') return this.string(start, start + 1, { close: [BT], escapes: true, multiline: true, interpolation: '${' });
      if (f === 'go') return this.string(start, start + 1, { close: [BT], escapes: false, multiline: true });
      return start;
    }
    const triple = b[start + 1] === q && b[start + 2] === q;
    if (f === 'python') {
      const prefix = this.wordBefore(start).toLowerCase();
      const flags = PYTHON_PREFIXES.has(prefix) ? prefix : '';
      const interpolation = flags.includes('f') ? '{' : null;
      if (triple) return this.string(start, start + 3, { close: [q, q, q], escapes: true, multiline: true, interpolation });
      return this.string(start, start + 1, { close: [q], escapes: true, multiline: false, interpolation });
    }
    if (q === SQ) {
      if (f === 'js') return this.string(start, start + 1, { close: [SQ], escapes: true, multiline: false });
      return this.charLiteral(start);
    }
    if (f === 'c' && CPP_RAW.has(this.wordBefore(start))) {
      const open = b.indexOf(LP, start + 1);
      const delimiter = open > start ? b.toString('latin1', start + 1, open) : null;
      if (delimiter != null && delimiter.length <= 16 && !/[\s\\]/.test(delimiter)) {
        const end = b.indexOf(`)${delimiter}"`, open, 'latin1');
        const stop = end < 0 ? b.length : end + delimiter.length + 2;
        this.mark(start, stop);
        return stop;
      }
    }
    if (f === 'rust') {
      let hashes = 0;
      while (b[start - 1 - hashes] === HASH) hashes += 1;
      if (RUST_RAW.has(this.wordBefore(start - hashes))) {
        const end = b.indexOf(`"${'#'.repeat(hashes)}`, start + 1, 'latin1');
        const stop = end < 0 ? b.length : end + 1 + hashes;
        this.mark(start, stop);
        return stop;
      }
      return this.string(start, start + 1, { close: [DQ], escapes: true, multiline: true });
    }
    if (f === 'csharp') {
      let i = start;
      let verbatim = false;
      let interpolated = false;
      while (b[i - 1] === DOLLAR || b[i - 1] === AT) {
        i -= 1;
        if (b[i] === AT) verbatim = true;
        else interpolated = true;
      }
      if (triple) {
        let quotes = 0;
        while (b[start + quotes] === DQ) quotes += 1;
        const end = b.indexOf('"'.repeat(quotes), start + quotes, 'latin1');
        const stop = end < 0 ? b.length : end + quotes;
        this.mark(start, stop);
        return stop;
      }
      return this.string(start, start + 1, { close: [DQ], escapes: !verbatim, doubled: verbatim, multiline: verbatim, interpolation: interpolated ? '{' : null });
    }
    if (f === 'scala') {
      const interpolation = this.wordBefore(start) ? '${' : null;
      if (triple) return this.string(start, start + 3, { close: [DQ, DQ, DQ], escapes: false, multiline: true, interpolation, greedyClose: true });
      return this.string(start, start + 1, { close: [DQ], escapes: true, multiline: false, interpolation });
    }
    if (f === 'java' && triple) return this.string(start, start + 3, { close: [DQ, DQ, DQ], escapes: true, multiline: true });
    return this.string(start, start + 1, { close: [DQ], escapes: true, multiline: false });
  }

  /** A char literal ('x' or an escape) at start; a quote opening none (Rust lifetime, digit separator) is code. */
  charLiteral(start) {
    const { b } = this;
    if (b[start + 1] === BS) {
      const end = b.indexOf(SQ, start + 3);
      const line = this.lineEnd(start);
      if (end < 0 || end > line) return start;
      this.mark(start, end + 1);
      return end + 1;
    }
    const width = utf8Length(b[start + 1] ?? 0);
    if (b[start + 1] === undefined || b[start + 1] === NL || b[start + 1 + width] !== SQ) return start;
    this.mark(start, start + 2 + width);
    return start + 2 + width;
  }

  /** A string body from bodyStart to its close; interpolated expressions inside it stay code. */
  string(start, bodyStart, { close, escapes, multiline, interpolation = null, doubled = false, greedyClose = false }) {
    const { b } = this;
    let segment = start;
    let i = bodyStart;
    const closes = (at) => close.every((byte, k) => b[at + k] === byte);
    while (i < b.length) {
      const c = b[i];
      if (escapes && c === BS) {
        i += 2;
        continue;
      }
      if (c === NL && !multiline) break;
      if (doubled && c === close[0] && b[i + 1] === close[0]) {
        i += 2;
        continue;
      }
      if (closes(i)) {
        i += close.length;
        while (greedyClose && b[i] === close[0]) i += 1;
        break;
      }
      if (interpolation === '{' && c === LB) {
        if (b[i + 1] === LB) {
          i += 2;
          continue;
        }
        this.mark(segment, i + 1);
        i = this.code(i + 1, true);
        segment = i;
        i += 1;
        continue;
      }
      if (interpolation === '${' && c === DOLLAR && b[i + 1] === LB) {
        this.mark(segment, i + 2);
        i = this.code(i + 2, true);
        segment = i;
        i += 1;
        continue;
      }
      if (interpolation === '${' && c === DOLLAR && b[i + 1] === DOLLAR) {
        i += 2;
        continue;
      }
      i += 1;
    }
    this.mark(segment, i);
    return Math.max(i, start + 1);
  }
}

/** A mask over the file's bytes: 1 where a string or comment covers the byte. Null for an extension with no lexer. */
export function nonCodeMask(bytes, ext) {
  const family = familyOf(ext);
  if (!family) return null;
  const scanner = new Scanner(Buffer.isBuffer(bytes) ? bytes : Buffer.from(bytes), family);
  scanner.code(0, false);
  return scanner.mask;
}
