// The search index over one saved graph (pure: the CLI, MCP and the viewer's browser worker share it). One document
// per file and per symbol, with fields: name (identifier words, camelCase/snake_case/kebab split, plus the whole name),
// parent (the class or function it is declared in), path (its file's path segments), kind, doc (the comment above
// the declaration, or a Python docstring), uses (names it calls or imports) and sig (parameter and return type text).
// Ranking is field-weighted BM25F (Robertson and Zaragoza), over Porter-stemmed words.
//
// Doc text and the declarations data.json leaves out (it keeps call-graph symbols only) come from the sidecar
// search-index.json, which Node builds from the source (search-sidecar.ts) and attaches with attachSearchIndex; the
// browser worker attaches it when the export embeds it. Without one, the index is built from data.json alone: every
// field but doc, over its call-graph symbols.
import type { QueryData } from './query-index.ts';
import { fileOf } from '../ids.ts';

/** A declaration as the index reads it: a data.json symbol, or a declaration the sidecar adds. */
export interface SearchSymbol {
  id: string;
  file: string;
  name: string;
  kind: string;
  line?: number | null;
  endLine?: number | null;
  exported?: boolean;
  /** Parameter and return type text as written (engine `params`/`returns`); absent: not recorded. */
  params?: string;
  returns?: string;
}

/** search-index.json: what the source adds to data.json for search, keyed by the data.json it was built from. */
export interface SearchSidecar {
  version: number;
  /** sha256 of the data.json bytes it was built from: any other data.json makes it stale. */
  data: string;
  /** Declarations the build's chunks hold that data.json's call-graph symbols do not. */
  symbols: SearchSymbol[];
  /** The comment above each declaration (or its Python docstring) by symbol id; a file's leading comment by file id. */
  docs: Record<string, string>;
  /** Files whose source was not read for doc text, and why (changed since the graph was written, unreadable). */
  unread: { file: string; reason: string }[];
}

export const SIDECAR_VERSION = 1;

/** A sidecar, nothing, or a call that reads (or builds) one on first use. */
export type SidecarSource = SearchSidecar | null | (() => SearchSidecar | null);

const sources = new WeakMap<object, SidecarSource>();

/** Gives `D` its sidecar: a value, or a call made once, on the first search. */
export function attachSearchIndex(D: QueryData, source: SidecarSource) {
  sources.set(D, source);
  indexes.delete(D);
}

// ---------------------------------------------------------------------------------------------------------------
// Words

// camelCase, PascalCase, snake_case, kebab-case, dotted and path names → lower-case words: a word ends at any
// character but an ASCII letter or digit, before an upper-case letter that follows a lower-case letter or digit
// (`parseArgs`), and before the last upper-case letter of a run that a lower-case letter follows (`HTTPServer`).
export function words(text: string) {
  const out: string[] = [];
  let start = -1;
  const kind = (c: number) =>
    c >= 97 && c <= 122
      ? 1
      : c >= 65 && c <= 90
        ? 2
        : c >= 48 && c <= 57
          ? 3
          : 0;
  for (let i = 0; i <= text.length; i++) {
    const k = i < text.length ? kind(text.charCodeAt(i)) : 0;
    if (!k) {
      if (start >= 0) out.push(text.slice(start, i).toLowerCase());
      start = -1;
      continue;
    }
    if (start < 0) {
      start = i;
      continue;
    }
    const prev = kind(text.charCodeAt(i - 1));
    const next = i + 1 < text.length ? kind(text.charCodeAt(i + 1)) : 0;
    if (k === 2 && (prev === 1 || prev === 3 || (prev === 2 && next === 1))) {
      out.push(text.slice(start, i).toLowerCase());
      start = i;
    }
  }
  return out;
}

/** English words that carry no subject in a question or a comment. */
export const STOP_WORDS = new Set(
  'a about above after again all also an and any are as at be been before being below between both but by can could did do does doing down during each either else few for from further had has have having here how i if in into is it its itself just let may me might more most must my no nor not now of off on once only or other our out over own per same shall she should so some such than that the their them then there these they this those through thus to too under until up upon us very via was we were what when where whether which while who whom whose why will with within without would yet you your'.split(
    ' '
  )
);

const VOWEL = /[aeiou]/;
// Porter's consonant-vowel measure of a stem: the number of VC sequences.
function measure(base: string) {
  let m = 0,
    prevVowel = false;
  for (let i = 0; i < base.length; i++) {
    const c = base[i];
    const vowel: boolean = VOWEL.test(c) || (c === 'y' && i > 0 && !prevVowel);
    if (!vowel && prevVowel) m++;
    prevVowel = vowel;
  }
  return m;
}
const hasVowel = (base: string) => /[aeiou]|(?<=.)y/.test(base);
// Ends consonant-vowel-consonant, the last not w, x or y (`hop` → `hope`, never `bow`).
const cvc = (s: string) => /[^aeiou][aeiouy][^aeiouwxy]$/.test(s);

/** Porter's step 1 (plurals, -ed, -ing, final y): `retries`/`retry` → `retri`, `counted` → `count`. */
export function stem(word: string) {
  if (word.length < 4 || /\d/.test(word)) return word;
  let w = word;
  if (w.endsWith('sses')) w = w.slice(0, -2);
  else if (w.endsWith('ies')) w = w.slice(0, -2);
  else if (w.endsWith('s') && !/(ss|us|is)$/.test(w)) w = w.slice(0, -1);
  let step = false;
  if (w.endsWith('eed')) {
    if (measure(w.slice(0, -3)) > 0) w = w.slice(0, -1);
  } else if (w.endsWith('ed') && hasVowel(w.slice(0, -2))) {
    w = w.slice(0, -2);
    step = true;
  } else if (w.endsWith('ing') && hasVowel(w.slice(0, -3))) {
    w = w.slice(0, -3);
    step = true;
  }
  if (step) {
    if (/(at|bl|iz)$/.test(w)) w += 'e';
    else if (/([^aeiouslz])\1$/.test(w)) w = w.slice(0, -1);
    else if (measure(w) === 1 && cvc(w)) w += 'e';
  }
  if (/[^aeiou]y$/.test(w) && w.length > 2) w = `${w.slice(0, -1)}i`;
  // Step 5a: a final e goes, so `parse` and `parsed` meet.
  if (w.endsWith('e')) {
    const base = w.slice(0, -1);
    const m = measure(base);
    if (m > 1 || (m === 1 && !cvc(base))) w = base;
  }
  return w;
}

/** The index terms of identifier or prose text: split words, stop words dropped from prose, stemmed. */
export function terms(text: string, prose = false) {
  const out: string[] = [];
  for (const w of words(text)) {
    if (prose && (STOP_WORDS.has(w) || w.length < 2 || /^\d+$/.test(w)))
      continue;
    out.push(stem(w));
  }
  return out;
}

const compact = (text: string) => text.toLowerCase().replace(/[^a-z0-9]/g, '');

// ---------------------------------------------------------------------------------------------------------------
// Documents

export const FIELDS = ['name', 'parent', 'kind', 'doc', 'uses', 'sig'] as const;
export type Field = (typeof FIELDS)[number] | 'path';

/** BM25F field weights and length normalization: a name word says more than a word in a comment. */
export const FIELD_WEIGHT: Record<Field, number> = {
  name: 3,
  parent: 1,
  path: 1,
  kind: 0.3,
  doc: 1.5,
  uses: 0.5,
  sig: 1,
};
const FIELD_B: Record<Field, number> = {
  name: 0.5,
  parent: 0.5,
  path: 0.5,
  kind: 0,
  doc: 0.75,
  uses: 0.75,
  sig: 0.5,
};
const K1 = 1.2;

/** One searchable thing: a file, or a symbol of a graph file. */
export interface SearchDoc {
  id: string;
  file: string;
  /** The file index (SearchIndex.files). */
  at: number;
  name: string;
  kind: string;
  symbol: boolean;
  line: number | null;
  endLine: number | null;
  exported: boolean;
  params?: string;
  returns?: string;
  /** In data.json's call-graph symbols (false: a declaration only the sidecar lists). */
  called: boolean;
}

interface Postings {
  /** termId → [offsets[t], offsets[t + 1]) in docs/tfs. */
  offsets: Int32Array;
  docs: Int32Array;
  tfs: Float32Array;
}

/** The BM25F index of one saved graph and its sidecar. */
export interface SearchIndex {
  docs: SearchDoc[];
  files: string[];
  /** file index → its documents (the file's own and its symbols'). */
  fileDocs: number[][];
  /** Term text by id, and id by text. */
  terms: string[];
  termId: Map<string, number>;
  /** A surface word of each term, for display (its stem may not be a word). */
  surface: string[];
  fields: Record<
    Exclude<Field, 'path'>,
    Postings & { length: Float32Array; avg: number }
  >;
  /** Path words are a file's: postings over file indexes, read through fileDocs. */
  path: Postings & { length: Float32Array; avg: number };
  /** Each document's distinct content terms (name, parent, doc, uses, sig): docTermOffsets over docTerms. */
  docTermOffsets: Int32Array;
  docTerms: Int32Array;
  /** How the doc text was found: read from a sidecar, none attached, or the sidecar failed to load. */
  sidecar: 'attached' | 'none' | 'failed';
  sidecarError?: string;
  /** Documents with doc text, and files whose source was not read for it. */
  documented: number;
  unread: SearchSidecar['unread'];
  /** Whether any symbol records params or returns (F12 filters need them). */
  signatures: boolean;
}

const indexes = new WeakMap<object, SearchIndex>();

function readSidecar(D: QueryData): {
  sidecar: SearchSidecar | null;
  error?: string;
} {
  const source = sources.get(D);
  if (!source) return { sidecar: null };
  try {
    return { sidecar: typeof source === 'function' ? source() : source };
  } catch (error) {
    return {
      sidecar: null,
      error: error instanceof Error ? error.message : String(error),
    };
  }
}

// Every symbol data.json knows (declared rows, plus call endpoints it does not declare), then the sidecar's
// declarations: one document each, for symbols of graph files.
function symbolsOf(D: QueryData, sidecar: SearchSidecar | null) {
  const byId = new Map<string, SearchSymbol & { called: boolean }>();
  for (const s of D.symbols ?? [])
    byId.set(s.id, {
      ...(s as SearchSymbol),
      file: s.file ?? fileOf(s.id),
      name: s.name ?? s.id.split('#').slice(1).join('#'),
      called: true,
    });
  for (const c of D.calls ?? [])
    for (const id of [c.source, c.target])
      if (!byId.has(id) && id.includes('#'))
        byId.set(id, {
          id,
          file: fileOf(id),
          name: id.split('#').slice(1).join('#').split('.').pop() ?? id,
          kind: 'symbol',
          line: null,
          called: true,
        });
  for (const s of sidecar?.symbols ?? [])
    if (!byId.has(s.id)) byId.set(s.id, { ...s, called: false });
  return byId;
}

/** The qualifier a symbol is declared in: `Atomic` for `db/transaction.py#Atomic.__exit__`. */
export function parentOf(id: string, name: string) {
  const qualified = id.slice(id.indexOf('#') + 1).replace(/@\d+$/, '');
  if (qualified === name) return '';
  if (qualified.endsWith(`.${name}`))
    return qualified.slice(0, -(name.length + 1));
  return qualified.slice(0, Math.max(0, qualified.lastIndexOf('.')));
}

const basename = (id: string) => id.slice(id.lastIndexOf('/') + 1);
const fileStem = (id: string) => basename(id).replace(/\.[^.]+$/, '');

// Term ids, and a surface word per term for display: the stem itself once it is seen as a word, else the first word
// that stemmed to it. Words already seen skip the stemmer.
class TermTable {
  readonly ids = new Map<string, number>();
  readonly list: string[] = [];
  readonly surface: string[] = [];
  private readonly byWord = new Map<string, number>();
  id(term: string, surface = term) {
    let t = this.ids.get(term);
    if (t === undefined) {
      t = this.list.length;
      this.ids.set(term, t);
      this.list.push(term);
      this.surface.push(surface);
    } else if (surface === term) this.surface[t] = term;
    return t;
  }
  /** The term of a lower-case word. */
  word(w: string) {
    let t = this.byWord.get(w);
    if (t === undefined) {
      t = this.id(stem(w), w);
      this.byWord.set(w, t);
    }
    return t;
  }
}

// Field terms of text: its words (stop words, single letters and numbers dropped from prose), as term ids.
function fieldTerms(table: TermTable, text: string, prose: boolean) {
  const out: number[] = [];
  for (const w of words(text)) {
    if (prose && (STOP_WORDS.has(w) || w.length < 2 || /^\d+$/.test(w)))
      continue;
    out.push(table.word(w));
  }
  return out;
}

// Postings of one field, collected document by document: (document, term, count) for each distinct term.
class Collector {
  doc: number[] = [];
  term: number[] = [];
  tf: number[] = [];
  length: number[] = [];
  add(d: number, ids: number[]) {
    this.length[d] = ids.length;
    if (!ids.length) return;
    const sorted = ids.length > 1 ? ids.toSorted((a, b) => a - b) : ids;
    for (let i = 0; i < sorted.length;) {
      let j = i + 1;
      while (j < sorted.length && sorted[j] === sorted[i]) j++;
      this.doc.push(d);
      this.term.push(sorted[i]);
      this.tf.push(j - i);
      i = j;
    }
  }
  build(termCount: number, docCount: number) {
    const offsets = new Int32Array(termCount + 1);
    for (const t of this.term) offsets[t + 1]++;
    for (let t = 0; t < termCount; t++) offsets[t + 1] += offsets[t];
    const fill = offsets.slice(0, termCount);
    const docs = new Int32Array(this.term.length);
    const tfs = new Float32Array(this.term.length);
    for (let k = 0; k < this.term.length; k++) {
      const at = fill[this.term[k]]++;
      docs[at] = this.doc[k];
      tfs[at] = this.tf[k];
    }
    // The average length over the documents that have the field: most symbols have no doc comment, and counting
    // them would make every comment look long.
    const length = new Float32Array(docCount);
    let total = 0,
      filled = 0;
    for (let d = 0; d < docCount; d++) {
      length[d] = this.length[d] ?? 0;
      total += length[d];
      if (length[d]) filled++;
    }
    return { offsets, docs, tfs, length, avg: total / Math.max(1, filled) };
  }
}

function buildIndex(D: QueryData): SearchIndex {
  const { sidecar, error } = readSidecar(D);
  const docText = sidecar?.docs ?? {};
  const files = (D.files ?? []).map(f => f.id);
  const fileAt = new Map(files.map((id, i) => [id, i]));
  const table = new TermTable();
  const docs: SearchDoc[] = [];
  const fileDocs: number[][] = files.map(() => []);
  const collect = Object.fromEntries(
    FIELDS.map(f => [f, new Collector()])
  ) as Record<(typeof FIELDS)[number], Collector>;
  const pathCollect = new Collector();
  const forward: number[][] = [];
  let documented = 0;

  // Names each symbol calls and each file imports or takes, for the uses field.
  const calls = new Map<string, string[]>();
  for (const c of D.calls ?? [])
    if (!c.test) {
      const list = calls.get(c.source) ?? [];
      list.push(c.target.slice(c.target.indexOf('#') + 1));
      calls.set(c.source, list);
    }
  const fileUses = new Map<string, string[]>();
  const useOf = (source: string, name: string) => {
    const list = fileUses.get(source) ?? [];
    list.push(name);
    fileUses.set(source, list);
  };
  for (const u of D.uses ?? [])
    useOf(u.source, u.target.slice(u.target.indexOf('#') + 1));
  for (const e of D.imports ?? []) useOf(e.source, fileStem(e.target));

  const addDoc = (doc: SearchDoc, fieldText: Record<string, number[]>) => {
    const d = docs.length;
    docs.push(doc);
    fileDocs[doc.at].push(d);
    const content: number[] = [];
    for (const f of FIELDS) {
      const ids = fieldText[f] ?? [];
      collect[f].add(d, ids);
      if (f !== 'kind') for (const t of ids) content.push(t);
    }
    content.sort((a, b) => a - b);
    forward.push(content.filter((t, i) => !i || t !== content[i - 1]));
  };

  files.forEach((id, at) => {
    pathCollect.add(at, fieldTerms(table, id.replace(/\.[^./]+$/, ''), false));
    const doc = docText[id];
    if (doc) documented++;
    const name = fileStem(id);
    addDoc(
      {
        id,
        file: id,
        at,
        name,
        kind: 'file',
        symbol: false,
        line: null,
        endLine: null,
        exported: false,
        called: true,
      },
      {
        name: [
          ...fieldTerms(table, name, false),
          table.id(compact(basename(id))),
        ],
        kind: [table.id('file')],
        doc: doc ? fieldTerms(table, doc, true) : [],
        uses: fieldTerms(table, (fileUses.get(id) ?? []).join(' '), false),
      }
    );
  });

  let signatures = false;
  for (const s of symbolsOf(D, sidecar).values()) {
    const at = fileAt.get(s.file);
    if (at === undefined) continue;
    const short = s.name.split('.').pop() ?? s.name;
    const doc = docText[s.id];
    if (doc) documented++;
    if (s.params != null || s.returns != null) signatures = true;
    const nameTerms = fieldTerms(table, short, false);
    const whole = compact(short);
    if (whole && (nameTerms.length !== 1 || table.list[nameTerms[0]] !== whole))
      nameTerms.push(table.id(whole));
    addDoc(
      {
        id: s.id,
        file: s.file,
        at,
        name: short,
        kind: s.kind ?? 'symbol',
        symbol: true,
        line: s.line ?? null,
        endLine: s.endLine ?? null,
        exported: !!s.exported,
        ...(s.params != null && { params: s.params }),
        ...(s.returns != null && { returns: s.returns }),
        called: s.called,
      },
      {
        name: nameTerms,
        parent: fieldTerms(table, parentOf(s.id, short), false),
        kind: [table.id(stem(s.kind ?? 'symbol'))],
        doc: doc ? fieldTerms(table, doc, true) : [],
        uses: fieldTerms(table, (calls.get(s.id) ?? []).join(' '), false),
        sig: fieldTerms(table, `${s.params ?? ''} ${s.returns ?? ''}`, false),
      }
    );
  }

  const termCount = table.list.length;
  const fields = Object.fromEntries(
    FIELDS.map(f => [f, collect[f].build(termCount, docs.length)])
  ) as SearchIndex['fields'];
  const path = pathCollect.build(termCount, files.length);
  const docTermOffsets = new Int32Array(docs.length + 1);
  forward.forEach((ids, d) => {
    docTermOffsets[d + 1] = docTermOffsets[d] + ids.length;
  });
  const docTerms = new Int32Array(docTermOffsets[docs.length]);
  forward.forEach((ids, d) => docTerms.set(ids, docTermOffsets[d]));
  return {
    docs,
    files,
    fileDocs,
    terms: table.list,
    termId: table.ids,
    surface: table.surface,
    fields,
    path,
    docTermOffsets,
    docTerms,
    sidecar: sidecar ? 'attached' : error ? 'failed' : 'none',
    ...(error && { sidecarError: error }),
    documented,
    unread: sidecar?.unread ?? [],
    signatures,
  };
}

/** The search index of `D`, built on first use (with its sidecar when one is attached). */
export function searchIndexOf(D: QueryData): SearchIndex {
  let index = indexes.get(D);
  if (!index) {
    index = buildIndex(D);
    indexes.set(D, index);
  }
  return index;
}

// ---------------------------------------------------------------------------------------------------------------
// Ranking

/** A query term and how much it counts (an original word 1; an expansion less). */
export interface QueryTerm {
  term: number;
  weight: number;
}

/** The query's terms in the index: words (stop words dropped unless nothing else is left), stemmed, plus the whole
 * text as one compact name when it is one identifier. Unknown words are listed apart. */
export function queryTerms(index: SearchIndex, text: string) {
  const all = words(text);
  const kept = all.filter(w => !STOP_WORDS.has(w));
  const chosen = kept.length ? kept : all;
  const found: QueryTerm[] = [];
  const unknown: string[] = [];
  const seen = new Set<number>();
  const add = (term: string, original: string) => {
    const id = index.termId.get(term);
    if (id === undefined) unknown.push(original);
    else if (!seen.has(id)) {
      seen.add(id);
      found.push({ term: id, weight: 1 });
    }
  };
  for (const w of chosen) add(stem(w), w);
  const whole = compact(text);
  if (!/\s/.test(text.trim()) && all.length > 1 && index.termId.has(whole))
    add(whole, text);
  return { terms: found, unknown };
}

/** BM25F scores of every document holding a query term (document → score), and which terms each one holds. */
export function bm25f(index: SearchIndex, query: QueryTerm[]) {
  const n = index.docs.length;
  const scores = new Map<number, number>();
  const matched = new Map<number, number[]>();
  const acc = new Float64Array(n);
  for (const { term, weight } of query) {
    const touched: number[] = [];
    const add = (d: number, v: number) => {
      if (!acc[d]) touched.push(d);
      acc[d] += v;
    };
    for (const f of FIELDS) {
      const p = index.fields[f];
      const w = FIELD_WEIGHT[f],
        b = FIELD_B[f];
      if (!w) continue;
      for (let k = p.offsets[term]; k < p.offsets[term + 1]; k++) {
        const d = p.docs[k];
        add(d, (w * p.tfs[k]) / (1 - b + (b * p.length[d]) / (p.avg || 1)));
      }
    }
    const p = index.path;
    for (let k = p.offsets[term]; k < p.offsets[term + 1]; k++) {
      const at = p.docs[k];
      const v =
        (FIELD_WEIGHT.path * p.tfs[k]) /
        (1 - FIELD_B.path + (FIELD_B.path * p.length[at]) / (p.avg || 1));
      for (const d of index.fileDocs[at]) add(d, v);
    }
    const df = touched.length;
    if (!df) continue;
    const idf = Math.log(1 + (n - df + 0.5) / (df + 0.5));
    for (const d of touched) {
      const tf = acc[d];
      acc[d] = 0;
      scores.set(d, (scores.get(d) ?? 0) + (weight * idf * tf) / (K1 + tf));
      const list = matched.get(d);
      if (list) list.push(term);
      else matched.set(d, [term]);
    }
  }
  return { scores, matched };
}

/** Documents ordered by score, best first (ties by document order, which is file then declaration order). */
export function ranked(scores: Map<number, number>) {
  return [...scores].sort((a, b) => b[1] - a[1] || a[0] - b[0]).map(e => e[0]);
}
