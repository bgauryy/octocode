// The battery's text oracle, as pure functions over ripgrep matches: which hits are call sites
// (not the probe's own declarations, not excluded files, not strings or comments), file-level
// recall/precision against the graph, and the per-site reasons behind each missed file.
import { matchesGlob } from 'node:path';

// Per-extension lexical rules; every delimiter is ASCII, so scanning UTF-8 bytes is exact.
// quotes: string delimiters; multiline: delimiters whose strings may cross a raw newline;
// triple: """/''' strings; line/block: comment forms. A quote missing here is ordinary text.
const C_FAMILY = { quotes: ['"', "'"], multiline: [], triple: false, line: '//', block: true };
const LEXICON = {
  js: { ...C_FAMILY, quotes: ['"', "'", '`'], multiline: ['`'] },
  ts: { ...C_FAMILY, quotes: ['"', "'", '`'], multiline: ['`'] },
  tsx: { ...C_FAMILY, quotes: ['"', "'", '`'], multiline: ['`'] },
  py: { quotes: ['"', "'"], multiline: [], triple: true, line: '#', block: false },
  go: { ...C_FAMILY, quotes: ['"', "'", '`'], multiline: ['`'] },
  // Rust lifetimes and Scala symbols use a lone `'`; only their char literals are skipped.
  rs: { ...C_FAMILY, quotes: ['"'], chars: true },
  scala: { ...C_FAMILY, quotes: ['"'], triple: true, chars: true },
};
const lexicon = (ext) => LEXICON[ext] ?? C_FAMILY;
const CHAR_LITERAL = /^'(?:\\.[^']{0,8}|[^\\'\n])'/s;

// Sorted [start, end) byte ranges of string literals and comments in one file.
export function nonCodeRanges(buf, ext) {
  const lex = lexicon(ext);
  const text = buf.toString('latin1'); // one char per byte: indices are byte offsets
  const ranges = [];
  let i = 0;
  const until = (end, from, escapes, stopAtNewline) => {
    let j = from;
    while (j < text.length) {
      if (escapes && text[j] === '\\') j += 2;
      else if (text.startsWith(end, j)) return j + end.length;
      else if (stopAtNewline && text[j] === '\n') return j;
      else j++;
    }
    return text.length;
  };
  while (i < text.length) {
    const ch = text[i];
    let end = -1;
    if (text.startsWith(lex.line, i)) end = until('\n', i, false, false);
    else if (lex.block && text.startsWith('/*', i)) end = until('*/', i + 2, false, false);
    else if (lex.triple && (text.startsWith('"""', i) || text.startsWith("'''", i))) end = until(text.slice(i, i + 3), i + 3, true, false);
    else if (lex.quotes.includes(ch)) end = until(ch, i + 1, ch !== '`' || ext !== 'go', !lex.multiline.includes(ch));
    else if (lex.chars && ch === "'") {
      const literal = CHAR_LITERAL.exec(text.slice(i, i + 12));
      if (literal) end = i + literal[0].length;
    }
    if (end > i) {
      ranges.push([i, end]);
      i = end;
    } else i++;
  }
  return ranges;
}

export function inRanges(ranges, offset) {
  let lo = 0, hi = ranges.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (ranges[mid][1] <= offset) lo = mid + 1;
    else hi = mid;
  }
  return lo < ranges.length && ranges[lo][0] <= offset;
}

export const excludedBy = (globs) => (file) => globs.some((glob) => matchesGlob(file, glob));

// matches: [{file, line, offset}] with `offset` the match's absolute byte offset; definitions:
// [{file, startByte}] name selections of the probe's declarations; text(file) returns a Buffer.
export function oracleSites({ matches, definitions, exclude = [], text, ext }) {
  const declared = new Set(definitions.map((d) => `${d.file}\0${d.startByte}`));
  const excluded = excludedBy(exclude);
  const masks = new Map();
  const mask = (file) => {
    if (!masks.has(file)) masks.set(file, nonCodeRanges(text(file), file.split('.').pop() ?? ext));
    return masks.get(file);
  };
  const dropped = { definition: 0, excluded: 0, nonCode: 0 };
  const sites = [];
  for (const m of matches) {
    if (excluded(m.file)) dropped.excluded++;
    else if (declared.has(`${m.file}\0${m.offset}`)) dropped.definition++;
    else if (inRanges(mask(m.file), m.offset)) dropped.nonCode++;
    else sites.push({ file: m.file, line: m.line });
  }
  return { sites, dropped };
}

export function fileRecall(siteFiles, graphFiles) {
  const both = [...graphFiles].filter((f) => siteFiles.has(f)).length;
  return {
    rgFiles: siteFiles.size,
    graphFiles: graphFiles.size,
    both,
    recall: siteFiles.size ? +(both / siteFiles.size).toFixed(2) : null,
    precision: graphFiles.size ? +(both / graphFiles.size).toFixed(2) : null,
    missed: [...siteFiles].filter((f) => !graphFiles.has(f)).toSorted(),
    extra: [...graphFiles].filter((f) => !siteFiles.has(f)).toSorted(),
  };
}

// The callee's last name segment: `this.app.scene.mutateElement` → mutateElement.
export const leaf = (name) => String(name ?? '').split(/::|\.|#|->/).pop().replace(/[<(].*$/, '');

// Every oracle site in a missed file with the graph's unresolved row for it (same file and line,
// target leaf = probe); `reason: null` when the graph holds no unresolved row there.
export function missedSites({ missed, sites, unresolved, probe }) {
  const missedSet = new Set(missed);
  const rows = new Map();
  for (const r of unresolved) {
    if (leaf(r.targetName) !== probe) continue;
    const key = `${r.file}\0${r.line}`;
    if (!rows.has(key)) rows.set(key, r);
  }
  return sites
    .filter((s) => missedSet.has(s.file))
    .map((s) => {
      const r = rows.get(`${s.file}\0${s.line}`);
      return { file: s.file, line: s.line, reason: r?.reason ?? null, targetName: r?.targetName ?? null };
    });
}
