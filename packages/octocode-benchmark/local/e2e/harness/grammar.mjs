// Grammar sweep: every Tree-sitter language through astSearch / localFetch / localSearch / lspSearch.
import fs from 'node:fs';
import path from 'node:path';
import { collect, declarations, rowData } from './responses.mjs';
import { FIXTURES } from './paths.mjs';
import { checks, startServer, writeResults } from './mcp-client.mjs';

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
  CUDA: { ext: 'cu', rg: 'cuda', kind: 'function_definition', call: 'helper', names: ['LIMIT', 'helper', 'run'], alt: ['cuh'],
    src: 'const int LIMIT = 3;\n__device__ int helper(int n) { return n * 2; }\n__global__ void run() {\n  const char *s = "😀"; int value = helper((int)s[0] + LIMIT);\n}\n' },
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
  check(`${lang}: directory symbols`, !dirSym.isError && dirSym.rowErrors === 0, dirSym.text.slice(0, 100));
  if (l.call) {
    // Captures are opt-in (captureText); default rows carry line/column/value only.
    const m = await call('astSearch', { operation: 'match', path: dir, parser: ({ TypeScript: 'typescript', JavaScript: 'javascript', TSX: 'tsx', 'C++': 'cpp', 'C#': 'csharp', Assembly: 'asm' }[lang] ?? lang.toLowerCase()), rule: { pattern: `${l.call}($A)` }, includeCaptures: true });
    // A match row is {line, endLine?, value}; positions live on its captures.
    const matches = collect(rowData(m), o => typeof o.value === 'string' && typeof o.line === 'number' && o.metavarRanges);
    const srcLines = l.src.split('\n');
    const emojiLine = srcLines.findIndex(s => s.includes('😀')) + 1;
    const onEmoji = matches.find(x => x.line === emojiLine);
    // 1-based UTF-16 column on the $A capture: JS string indices count UTF-16 units.
    const capture = onEmoji?.metavarRanges?.A?.[0];
    const expected = emojiLine && capture ? srcLines[emojiLine - 1].indexOf(capture.text) + 1 : -1;
    res.match = `${matches.length} hits`;
    res.utf16 = capture ? (capture.column === expected ? `ok(${expected})` : `BAD ${capture.column}≠${expected}`) : 'no-hit';
    check(`${lang}: call pattern ${l.call}($A) matches`, matches.length > 0, m.text.slice(0, 100));
    check(`${lang}: UTF-16 column after emoji`, !!capture && capture.column === expected, res.utf16);
  }
  const k = await call('astSearch', { operation: 'match', path: dir, parser: ({ TypeScript: 'typescript', JavaScript: 'javascript', TSX: 'tsx', 'C++': 'cpp', 'C#': 'csharp', Assembly: 'asm' }[lang] ?? lang.toLowerCase()), rule: { kind: l.kind }, view: 'countMatches' });
  // Minimal countMatches: per-file matchCount (restored from `shared`).
  const kc = (rowData(k)?.files ?? []).reduce((sum, f) => sum + (f.matchCount ?? 0), 0);
  res.kind = `${l.kind}=${kc}`;
  check(`${lang}: kind rule ${l.kind}`, kc > 0, k.text.slice(0, 80));
  const st = await call('astSearch', { operation: 'syntaxTree', path: file, namedOnly: true });
  check(`${lang}: syntax tree contains source nodes`, !st.isError && !st.rowErrors && rowData(st)?.nodes?.some(node => typeof node === 'string' && node.includes(`1:1-${l.src.split('\n').length}:1`)), `${st.bytes}B`);
  const f = await call('localFetch', { path: file, view: 'symbols' });
  check(`${lang}: localFetch symbols view names ${l.names.at(-1)}`, (rowData(f)?.content ?? '').includes(l.names.at(-1)), rowData(f)?.view);
  const g = await call('localSearch', { path: dir, matchString: [l.names.at(-1)], fileType: l.rg, view: 'files' });
  check(`${lang}: localSearch language ${l.rg}`, (rowData(g)?.files ?? []).some(f => f.path.endsWith(`main.${l.ext}`)), g.text.slice(0, 80));

  for (const alt of l.alt) {
    const a = await call('astSearch', { operation: 'symbols', path: path.join(dir, `alt.${alt}`), ...(l.cppHeader && ['hpp', 'hh', 'hxx'].includes(alt) ? { parser: 'cpp' } : {}) });
    check(`${lang}: .${alt} symbols`, names(a).has(l.names.at(-1)), a.text.slice(0, 80));
  }
  table.push(res);
}
console.table(table);
const result = summary();
writeResults('grammar', { table, ...result });
client.close();
process.exitCode = result.failed.length ? 1 : 0;
