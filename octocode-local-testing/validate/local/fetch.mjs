// localFetch vs sed -n / rg -n -C / head / tail. Truth = the file bytes read by node.
import fs from 'node:fs';
import path from 'node:path';
import { REPOS, sh, oc, ocAll, saveRaw } from './lib.mjs';

const A = (p) => path.join(REPOS, p);
const lines = (f) => fs.readFileSync(A(f), 'utf8').split('\n');

// verify an octocode localFetch data block against the file: every returned line must equal the source line,
// and the covered set must equal the wanted set.
function verifyOc(data, file, want) {
  const L = lines(file);
  if (!data || data.error) return { ok: false, error: data?.error || 'no data' };
  const ranges = (data.sourceLineRanges || []).map(r => ({ start: r.line, end: r.endLine }));
  const chunks = (data.content || '').split(/\n?\.\.\. \[lines \d+-\d+ omitted\] \.\.\.\n?/);
  const covered = []; let mismatches = 0;
  ranges.forEach((r, i) => {
    const body = (chunks[i] ?? '').replace(/\n$/, '').split('\n');
    for (let ln = r.start; ln <= r.end; ln++) {
      covered.push(ln);
      if (body[ln - r.start] !== L[ln - 1]) mismatches++;
    }
  });
  const W = new Set(want), C = new Set(covered);
  const missing = want.filter(x => !C.has(x));
  const extra = covered.filter(x => !W.has(x));
  return { ok: mismatches === 0 && missing.length === 0, mismatches, covered: covered.length, want: want.length, missing: missing.length, extra: extra.length, citations: ranges };
}
function verifySed(out, file, want) {
  const L = lines(file); const got = out.replace(/\n$/, '').split('\n');
  const exp = want.map(n => L[n - 1]);
  return { ok: got.length === exp.length && got.every((x, i) => x === exp[i]), got: got.length, want: exp.length, citations: 'none (sed prints no line numbers; agent knows a,b from the command)' };
}
function verifyRgC(out, file, want) {
  const L = lines(file); const covered = []; let mism = 0;
  for (const l of out.split('\n')) { const m = l.match(/^(\d+)[:-]([\s\S]*)$/); if (m) { covered.push(+m[1]); if (m[2] !== L[+m[1] - 1]) mism++; } }
  const C = new Set(covered); const missing = want.filter(x => !C.has(x));
  return { ok: mism === 0 && missing.length === 0, mismatches: mism, covered: covered.length, want: want.length, missing: missing.length, citations: 'every line numbered' };
}
const range = (a, b) => Array.from({ length: b - a + 1 }, (_, i) => a + i);

const R = [
  ['ts', 'tsx/packages/element/src/newElement.ts', 174, 230],
  ['rust', 'rust/tokio/src/runtime/blocking/pool.rs', 429, 500],
  ['go', 'go/tsdb/head.go', 1250, 1300],
  ['python', 'python/django/db/models/query.py', 861, 960],
  ['java', 'java/guava/src/com/google/common/collect/Lists.java', 731, 745],
  ['c', 'c/src/server.c', 4151, 4250],
  ['cpp', 'cpp/single_include/nlohmann/json.hpp', 29863, 29900],
  ['huge-ts', 'typescript/tsc/testdata/fixtures/compiler/checker.ts', 49064, 49120],
];
const M = [
  ['ts', 'tsx/packages/element/src/newElement.ts', 'export const newTextElement'],
  ['rust', 'rust/tokio/src/runtime/blocking/pool.rs', 'fn spawn_thread'],
  ['go', 'go/tsdb/head.go', 'func (h *Head) gc()'],
  ['python', 'python/django/db/models/query.py', 'def get_or_create'],
  ['java', 'java/guava/src/com/google/common/collect/Lists.java', 'public static <E extends @Nullable Object> ArrayList<E> newArrayList()'],
  ['c', 'c/src/server.c', 'int processCommand(client *c)'],
  ['cpp', 'cpp/single_include/nlohmann/json.hpp', 'class lexer : public lexer_base'],
  ['huge-ts', 'typescript/tsc/testdata/fixtures/compiler/checker.ts', 'function checkSourceElementWorker'],
];
const out = [];
const log = (r) => console.log(`${r.id.padEnd(22)} shell ${r.shell.calls}x ${String(r.shell.chars).padStart(6)}c ${String(r.shell.ms).padStart(4)}ms ${r.shell.correct.ok ? 'OK ' : 'BAD'} | oc ${r.octocode.calls}x ${String(r.octocode.chars).padStart(6)}c ${String(r.octocode.ms).padStart(5)}ms ${r.octocode.correct.ok ? 'OK' : 'BAD ' + JSON.stringify(r.octocode.correct).slice(0, 160)}`);
const rec = (id, lang, task, shellCmd, s, sc, q, o, occ) => {
  const r = { id, lang, task, shell: { cmd: shellCmd, calls: s.calls || 1, chars: s.chars, ms: s.ms, correct: sc },
    octocode: { query: q, calls: o.calls || 1, chars: o.chars, ms: o.ms, correct: occ, steps: o.steps || [{ chars: o.chars, stdout: o.stdout }] } };
  out.push(r); log(r);
};

for (const [lang, f, a, b] of R) {
  const cmd = `sed -n '${a},${b}p' ${f}`;
  const s = sh(cmd, { cwd: REPOS });
  const q = { path: A(f), startLine: a, endLine: b };
  const o = oc('localFetch', q);
  rec(`range-${lang}`, lang, `Read lines ${a}-${b} of ${path.basename(f)}`, cmd, s, verifySed(s.stdout, f, range(a, b)), q, o, verifyOc(o.parsed?.results?.[0]?.data, f, range(a, b)));
}
for (const [lang, f, lit] of M) {
  const L = lines(f); const hits = L.map((x, i) => x.includes(lit) ? i + 1 : 0).filter(Boolean);
  const want = [...new Set(hits.flatMap(h => range(Math.max(1, h - 5), Math.min(L.length, h + 5))))].sort((x, y) => x - y);
  const cmd = `rg -n -C5 -F ${JSON.stringify(lit).replace(/\$/g, '\\$')} ${f}`;
  const s = sh(cmd, { cwd: REPOS });
  const q = { path: A(f), matchString: lit, matchStringCaseSensitive: true, contextLines: 5 };
  const o = oc('localFetch', q);
  rec(`match-${lang}`, lang, `Window ±5 around "${lit}" in ${path.basename(f)}`, cmd, s, verifyRgC(s.stdout, f, want), q, o, verifyOc(o.parsed?.results?.[0]?.data, f, want));
}
// head -40
for (const [lang, f] of [['ts', 'tsx/packages/element/src/newElement.ts'], ['go', 'go/tsdb/head.go'], ['c', 'c/src/server.c']]) {
  const cmd = `head -n 40 ${f}`; const s = sh(cmd, { cwd: REPOS });
  const q = { path: A(f), startLine: 1, endLine: 40 }; const o = oc('localFetch', q);
  rec(`head-${lang}`, lang, `First 40 lines of ${path.basename(f)}`, cmd, s, verifySed(s.stdout, f, range(1, 40)), q, o, verifyOc(o.parsed?.results?.[0]?.data, f, range(1, 40)));
}
// tail -30: octocode needs totalLines first (1-line probe) then the range; no negative offsets exist.
for (const [lang, f] of [['rust', 'rust/tokio/src/runtime/blocking/pool.rs'], ['python', 'python/django/db/models/query.py'], ['java', 'java/guava/src/com/google/common/collect/Lists.java']]) {
  const cmd = `tail -n 30 ${f}`; const s = sh(cmd, { cwd: REPOS });
  const L = lines(f); const n = L[L.length - 1] === '' ? L.length - 1 : L.length;
  const p1 = oc('localFetch', { path: A(f), startLine: 1, endLine: 1 });
  const total = p1.parsed?.results?.[0]?.data?.totalLines;
  const q = { path: A(f), startLine: total - 29, endLine: total }; const p2 = oc('localFetch', q);
  const o = { calls: 2, chars: p1.chars + p2.chars, ms: p1.ms + p2.ms, steps: [{ query: p1.query, chars: p1.chars, stdout: p1.stdout }, { query: p2.query, chars: p2.chars, stdout: p2.stdout }] };
  rec(`tail-${lang}`, lang, `Last 30 lines of ${path.basename(f)}`, cmd, s, verifySed(s.stdout, f, range(n - 29, n)), q, o, { ...verifyOc(p2.parsed?.results?.[0]?.data, f, range(n - 29, n)), totalLinesReported: total, wcLines: n });
}
// outline: minify symbols vs rg declaration grep (what an agent does to skim a file's shape)
for (const [lang, f, rx] of [
  ['ts', 'tsx/packages/element/src/newElement.ts', '^export (const|function|type|interface) \\w+'],
  ['go', 'go/tsdb/head.go', '^func '],
  ['python', 'python/django/db/models/query.py', '^\\s*(class|def) \\w+'],
]) {
  const cmd = `rg -n '${rx}' ${f}`; const s = sh(cmd, { cwd: REPOS });
  const q = { path: A(f), minify: 'symbols' }; const o = oc('localFetch', q);
  const truthCount = s.stdout.split('\n').filter(Boolean).length;
  rec(`outline-${lang}`, lang, `Skim declarations of ${path.basename(f)}`, cmd, s, { ok: true, declLines: truthCount }, q, o,
    { ok: !o.parsed?.results?.[0]?.data?.error, note: 'qualitative; see REPORT', contentChars: (o.parsed?.results?.[0]?.data?.content || '').length });
}
saveRaw('fetch', out);
