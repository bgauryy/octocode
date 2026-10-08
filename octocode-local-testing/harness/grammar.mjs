// Grammar sweep: every Tree-sitter language through astSearch / localFetch / localSearch / lspSearch.
import fs from 'node:fs';
import path from 'node:path';
import { FIXTURES, astMatchRows, checks, collect, declarations, rowData, startServer, writeResults } from './mcp-client.mjs';

const FIX = path.join(FIXTURES, 'grammar');
const { check, summary } = checks('grammar');
const L = {
  TypeScript: { ext: 'ts', rg: 'ts', kind: 'function_declaration', call: 'helper', names: ['LIMIT', 'Shape', 'Widget', 'helper', 'run'], alt: ['mts', 'cts'],
    src: 'export const LIMIT = 3;\nexport interface Shape { area(): number }\nexport class Widget implements Shape {\n  area(): number { return helper(LIMIT); }\n}\nexport function helper(n: number): number { return n * 2; }\nexport function run(): number {\n  const s = "😀"; return helper(s.length);\n}\n' },
  TSX: { ext: 'tsx', rg: 'ts', kind: 'function_declaration', call: 'helper', names: ['LIMIT', 'helper', 'Widget', 'run'], alt: [],
    src: 'export const LIMIT = 3;\nexport function helper(n: number) { return n * 2; }\nexport function Widget() {\n  const s = "😀"; return <div>{helper(s.length)}</div>;\n}\nexport function run() { return helper(LIMIT); }\n' },
  JavaScript: { ext: 'js', rg: 'js', kind: 'function_declaration', call: 'helper', names: ['LIMIT', 'Widget', 'helper', 'run'], alt: ['jsx', 'mjs', 'cjs'],
    src: 'const LIMIT = 3;\nclass Widget {\n  area() { return helper(LIMIT); }\n}\nfunction helper(n) { return n * 2; }\nfunction run() {\n  const s = "😀"; return helper(s.length);\n}\nmodule.exports = { Widget, helper, run };\n' },
  Python: { ext: 'py', rg: 'py', kind: 'function_definition', call: 'helper', names: ['LIMIT', 'Widget', 'area', 'helper', 'run'], alt: ['pyi'],
    src: 'LIMIT = 3\n\nclass Widget:\n    def area(self):\n        return helper(LIMIT)\n\ndef helper(n):\n    return n * 2\n\ndef run():\n    s = "😀"; return helper(len(s))\n' },
  Go: { ext: 'go', rg: 'go', kind: 'function_declaration', call: 'helper', names: ['LIMIT', 'Widget', 'Area', 'helper', 'run'], alt: [],
    src: 'package demo\n\nconst LIMIT = 3\n\ntype Widget struct{ n int }\n\nfunc (w Widget) Area() int { return helper(LIMIT) }\n\nfunc helper(n int) int { return n * 2 }\n\nfunc run() int {\n\ts := "😀"; return helper(len(s))\n}\n' },
  Rust: { ext: 'rs', rg: 'rust', kind: 'function_item', call: 'helper', names: ['LIMIT', 'Widget', 'area', 'helper', 'run'], alt: [],
    src: 'pub const LIMIT: i32 = 3;\npub struct Widget { n: i32 }\nimpl Widget {\n    pub fn area(&self) -> i32 { helper(LIMIT + self.n) }\n}\npub fn helper(n: i32) -> i32 { n * 2 }\npub fn run() -> i32 {\n    let s = "😀"; helper(s.len() as i32)\n}\n' },
  Java: { ext: 'java', rg: 'java', kind: 'method_declaration', call: 'helper', names: ['Widget', 'LIMIT', 'area', 'helper', 'run'], alt: [],
    src: 'public class Widget {\n  static final int LIMIT = 3;\n  int area() { return helper(LIMIT); }\n  static int helper(int n) { return n * 2; }\n  static int run() {\n    String s = "😀"; return helper(s.length());\n  }\n}\n' },
  C: { ext: 'c', rg: 'c', kind: 'function_definition', call: 'helper', names: ['LIMIT', 'Widget', 'helper', 'run'], alt: ['h'],
    src: '#define LIMIT 3\nstruct Widget { int n; };\nint helper(int n) { return n * 2; }\nint run(void) {\n  const char *s = "😀"; return helper((int)s[0]);\n}\n' },
  'C++': { ext: 'cpp', rg: 'cpp', kind: 'function_definition', call: 'helper', names: ['LIMIT', 'helper', 'Widget', 'area', 'run'], alt: ['hpp', 'cc', 'cxx', 'hh', 'hxx'], cppHeader: true,
    src: 'const int LIMIT = 3;\nint helper(int n) { return n * 2; }\nclass Widget {\n public:\n  int area() { return helper(LIMIT); }\n};\nint run() {\n  const char *s = "😀"; return helper(s[0]);\n}\n' },
  'C#': { ext: 'cs', rg: 'csharp', kind: 'method_declaration', call: 'Helper', names: ['Widget', 'LIMIT', 'Area', 'Helper', 'Run'], alt: [],
    src: 'public class Widget {\n  const int LIMIT = 3;\n  int Area() { return Helper(LIMIT); }\n  static int Helper(int n) { return n * 2; }\n  static int Run() {\n    var s = "😀"; return Helper(s.Length);\n  }\n}\n' },
  Scala: { ext: 'scala', rg: 'scala', kind: 'function_definition', call: 'helper', names: ['Widget', 'LIMIT', 'helper', 'run'], alt: ['sc', 'sbt'],
    src: 'object Widget {\n  val LIMIT = 3\n  def helper(n: Int): Int = n * 2\n  def run(): Int = {\n    val s = "😀"; helper(s.length)\n  }\n}\n' },
  Assembly: { ext: 'asm', rg: 'asm', kind: 'label', call: null, names: ['helper', 'run'], alt: ['s', 'assembly'],
    src: 'section .text\nglobal run\nhelper:\n    mov eax, edi\n    ret\nrun:\n    call helper\n    ret\n' },
};


for (const [, l] of Object.entries(L)) {
  const dir = path.join(FIX, l.ext);
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, `main.${l.ext}`), l.src);
  for (const alt of l.alt) fs.writeFileSync(path.join(dir, `alt.${alt}`), l.src);
}
const client = await startServer();
const { call } = client;
const names = entry => new Set(declarations(entry).filter(o => typeof o.name === 'string' && typeof o.line === 'number').map(o => o.name));
const table = [];
for (const [lang, l] of Object.entries(L)) {
  const dir = path.join(FIX, l.ext);
  const file = path.join(dir, `main.${l.ext}`);
  const res = { lang };
  const sym = await call('astSearch', { operation: 'symbols', path: file });
  const missing = l.names.filter(n => !names(sym).has(n));
  res.symbols = sym.isError ? 'ERR' : missing.length ? `miss:${missing}` : `ok(${names(sym).size})`;
  check(`${lang}: symbols has ${l.names.join(',')}`, !sym.isError && !missing.length, missing.join(','));
  const dirSym = await call('astSearch', { operation: 'symbols', path: dir });
  res.symbolsDir = dirSym.isError ? 'ERR' : `ok(${declarations(dirSym).filter(o => typeof o.name === 'string' && typeof o.line === 'number').length})`;
  check(`${lang}: directory symbols (no language)`, !dirSym.isError && dirSym.rowErrors === 0, dirSym.text.slice(0, 100));
  const bad = await call('astSearch', { operation: 'symbols', path: dir, language: lang });
  const repair = rowData(bad)?.hints?.repair;
  check(`${lang}: directory symbols with language returns an exact repair`, !!repair && !('language' in repair.query), JSON.stringify(rowData(bad)?.hints ?? {}).slice(0, 100));
  if (l.call) {
    // Captures are opt-in (captureText); default rows carry line/column/value only.
    const m = await call('astSearch', { operation: 'match', path: dir, language: lang, pattern: `${l.call}($A)`, captureText: true });
    const matches = collect(rowData(m), o => typeof o.value === 'string' && typeof o.line === 'number' && typeof o.column === 'number' && o.metavarRanges);
    const lean = await call('astSearch', { operation: 'match', path: dir, language: lang, pattern: `${l.call}($A)` });
    const leanRows = astMatchRows(lean);
    // A lean row hides a capture when its value does not show the capture
    // text; exactly then next.expandCaptures offers the captureText rows.
    const norm = t => t.split(/\s+/).filter(Boolean).join(' ');
    const hidden = matches.some((full, i) => Object.values(full.metavarRanges).flat().some(r => norm(r.text) && !(leanRows[i]?.value ?? '').includes(norm(r.text))));
    check(`${lang}: default match rows are lean and offer next.expandCaptures iff a value hides a capture`, leanRows.length === matches.length && leanRows.every((o, i) => typeof o.value === 'string' && o.line === matches[i].line && !o.metavarRanges && !o.metavars) && !!rowData(lean)?.next?.expandCaptures === hidden, `${leanRows.length} vs ${matches.length} hidden=${hidden}`);
    const srcLines = l.src.split('\n');
    const emojiLine = srcLines.findIndex(s => s.includes('😀')) + 1;
    const onEmoji = matches.find(x => x.line === emojiLine);
    // 1-based UTF-16 column (D2): JS string indices count UTF-16 units.
    const expected = emojiLine ? srcLines[emojiLine - 1].indexOf(`${l.call}(`) + 1 : -1;
    res.match = `${matches.length} hits`;
    res.utf16 = onEmoji ? (onEmoji.column === expected ? `ok(${expected})` : `BAD ${onEmoji.column}≠${expected}`) : 'no-hit';
    check(`${lang}: call pattern ${l.call}($A) matches`, matches.length > 0, m.text.slice(0, 100));
    check(`${lang}: UTF-16 column after emoji`, onEmoji && onEmoji.column === expected, res.utf16);
  }
  const k = await call('astSearch', { operation: 'match', path: dir, language: lang, rule: `rule:\n  kind: ${l.kind}\n`, resultView: 'countMatches' });
  // Minimal countMatches: per-file matchCount (restored from `shared`).
  const kc = (rowData(k)?.files ?? []).reduce((sum, f) => sum + (f.matchCount ?? 0), 0);
  res.kind = `${l.kind}=${kc}`;
  check(`${lang}: kind rule ${l.kind}`, kc > 0, k.text.slice(0, 80));
  const st = await call('astSearch', { operation: 'syntaxTree', path: file, namedOnly: true, pageSize: 15 });
  check(`${lang}: syntaxTree pages`, !st.isError && /next/i.test(st.text), `${st.bytes}B`);
  const f = await call('localFetch', { path: file, minify: 'symbols' });
  check(`${lang}: localFetch symbols view names ${l.names.at(-1)}`, (rowData(f)?.content ?? '').includes(l.names.at(-1)), rowData(f)?.contentView);
  const g = await call('localSearch', { path: dir, matchString: l.names.at(-1), language: l.rg, resultView: 'files' });
  check(`${lang}: localSearch language ${l.rg}`, (rowData(g)?.files ?? []).some(f => f.path.endsWith(`main.${l.ext}`)), g.text.slice(0, 80));
  const lsp = await call('lspSearch', { path: file, operation: 'documentSymbols' });
  res.lsp = lsp.isError || lsp.rowErrors ? `n/a(${rowData(lsp)?.errorCode})` : 'ok';
  for (const alt of l.alt) {
    const a = await call('astSearch', { operation: 'symbols', path: path.join(dir, `alt.${alt}`), ...(l.cppHeader && ['hpp', 'hh', 'hxx'].includes(alt) ? { language: 'cpp' } : {}) });
    check(`${lang}: .${alt} symbols`, names(a).has(l.names.at(-1)), a.text.slice(0, 80));
  }
  table.push(res);
}
console.table(table);
const result = summary();
writeResults('grammar', { table, ...result });
client.close();
process.exitCode = result.failed.length ? 1 : 0;
