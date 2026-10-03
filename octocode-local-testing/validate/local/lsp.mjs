// lspSearch vs rg -w. Truth sets were adjudicated by reading every rg -w hit (see REPORT.md "LSP truth").
import path from 'node:path';
import { REPOS, sh, oc, ocAll, pr, saveRaw } from './lib.mjs';

const A = (p) => path.join(REPOS, p);
function lspKeys(all, scopeDir) {
  const k = [];
  for (const p of all.parts) { if (!p) continue;
    for (const r of p.results || []) { const d = r.data || {};
      const locs = d.payload?.locations || [];
      for (const L of locs) { const rel = path.relative(REPOS, path.resolve(p.base || '', L.path || d.path || '')); if (!scopeDir || rel.startsWith(scopeDir)) k.push(`${rel}:${L.displayRange?.startLine}`); } } }
  return [...new Set(k)];
}
const rgK = (out) => [...new Set(out.split('\n').map(l => l.match(/^(.+?):(\d+):/)).filter(Boolean).map(m => `${m[1]}:${m[2]}`))];

const REFS = [
  { id: 'refs-rust', lang: 'rust', task: 'References to tokio::sync::Semaphore::add_permits (semaphore.rs:568)',
    shell: `rg -n -w add_permits -t rust rust`, scope: 'rust/',
    q: { operation: 'references', uri: A('rust/tokio/src/sync/semaphore.rs'), symbolName: 'add_permits', lineHint: 568, pageSize: 100 },
    truth: ['rust/tokio-util/tests/task_join_queue.rs:350', 'rust/tokio-util/tests/task_join_map.rs:295', 'rust/tokio-util/tests/task_join_map.rs:429',
      'rust/tokio-util/src/sync/poll_semaphore.rs:141', 'rust/tokio/tests/task_join_set.rs:309',
      ...[49, 58, 71, 205, 214, 222, 223, 237, 260].map(n => `rust/tokio/tests/sync_semaphore.rs:${n}`), 'rust/tokio/tests/sync_semaphore_owned.rs:75',
      'rust/tokio/src/sync/semaphore.rs:568', 'rust/tokio/src/sync/semaphore.rs:1403', 'rust/tokio/src/sync/semaphore.rs:1409'],
    // intra-doc links / doctests: neither side is penalised for these
    neutral: ['rust/tokio-util/src/sync/poll_semaphore.rs:136', 'rust/tokio-util/src/sync/poll_semaphore.rs:139', 'rust/tokio/src/sync/semaphore.rs:52', 'rust/tokio/src/sync/semaphore.rs:354', 'rust/tokio/src/sync/semaphore.rs:381'] },
  { id: 'refs-ts', lang: 'ts', task: 'References to exported getNonDeletedElements (element/src/index.ts:55), within packages/element/src',
    shell: `rg -n -w getNonDeletedElements tsx/packages/element/src`, scope: 'tsx/packages/element/src/',
    q: { operation: 'references', uri: A('tsx/packages/element/src/index.ts'), symbolName: 'getNonDeletedElements', lineHint: 55, pageSize: 100 },
    truth: ['tsx/packages/element/src/distribute.ts:8', 'tsx/packages/element/src/distribute.ts:37', 'tsx/packages/element/src/align.ts:7', 'tsx/packages/element/src/align.ts:29',
      'tsx/packages/element/src/index.ts:55', 'tsx/packages/element/src/frame.ts:36', 'tsx/packages/element/src/frame.ts:353'], neutral: [] },
  { id: 'refs-python', lang: 'python', task: 'References to django.utils.text.slugify (text.py:466)',
    shell: `rg -n -w slugify -t py python`, scope: 'python/',
    q: { operation: 'references', uri: A('python/django/utils/text.py'), symbolName: 'slugify', lineHint: 466, pageSize: 100 },
    truth: ['python/django/utils/text.py:466', 'python/django/template/defaultfilters.py:22', 'python/django/template/defaultfilters.py:274',
      'python/tests/utils_tests/test_text.py:371', 'python/tests/utils_tests/test_text.py:374'], neutral: [] },
  { id: 'refs-c', lang: 'c', task: 'References to zmalloc_used_memory (zmalloc.c:658); no compile_commands.json',
    shell: `rg -n -w zmalloc_used_memory -g '*.c' -g '*.h' c/src`, scope: 'c/',
    q: { operation: 'references', uri: A('c/src/zmalloc.c'), symbolName: 'zmalloc_used_memory', lineHint: 658, pageSize: 100 },
    truth: null /* computed below: every rg hit that is code (not comment), verified by reading */, neutral: [] },
  { id: 'refs-go', lang: 'go', task: 'References to tsdb.NewHead (gopls not installed)',
    shell: `rg -n -w NewHead -t go go`, scope: 'go/',
    q: { operation: 'references', uri: A('go/tsdb/head.go'), symbolName: 'NewHead', lineHint: 287, pageSize: 100 }, truth: null, neutral: [] },
  { id: 'refs-java', lang: 'java', task: 'References to Lists.partition (jdtls not installed)',
    shell: `rg -n -w 'partition' -t java java/guava/src`, scope: 'java/',
    q: { operation: 'references', uri: A('java/guava/src/com/google/common/collect/Lists.java'), symbolName: 'partition', lineHint: 731, pageSize: 100 }, truth: null, neutral: [] },
];

const DEFS = [
  { id: 'def-ts', lang: 'ts', task: 'Definition of getNonDeletedElements called at frame.ts:353',
    shellSteps: [`rg -n '(const|function) getNonDeletedElements\\b' -t ts tsx/packages/element/src`, `rg -n 'getNonDeletedElements' tsx/packages/element/src/frame.ts`],
    q: { operation: 'definition', uri: A('tsx/packages/element/src/frame.ts'), symbolName: 'getNonDeletedElements', lineHint: 353 },
    truth: 'tsx/packages/element/src/index.ts:55' },
  { id: 'def-python', lang: 'python', task: 'Definition of _slugify called at defaultfilters.py:274',
    shellSteps: [`rg -n -w '_slugify' python/django/template/defaultfilters.py`, `rg -n 'def slugify\\b' python/django/utils/text.py`],
    q: { operation: 'definition', uri: A('python/django/template/defaultfilters.py'), symbolName: '_slugify', lineHint: 274 },
    truth: 'python/django/utils/text.py:466' },
  { id: 'def-rust', lang: 'rust', task: 'Definition of semaphore.add_permits called at mpsc/bounded.rs:1789 (trait impl dispatch)',
    shellSteps: [`rg -n 'fn add_permits' -t rust rust/tokio/src`, `rg -n 'use chan::Semaphore|let semaphore' rust/tokio/src/sync/mpsc/bounded.rs`, `rg -n -B2 'fn add_permits' rust/tokio/src/sync/mpsc/chan.rs`, `rg -n 'impl Semaphore for' rust/tokio/src/sync/mpsc/chan.rs`],
    q: { operation: 'definition', uri: A('rust/tokio/src/sync/mpsc/bounded.rs'), symbolName: 'add_permits', lineHint: 1789 },
    truth: 'rust/tokio/src/sync/mpsc/chan.rs:582' },
];

const out = [];
for (const t of REFS) {
  const s = sh(t.shell, { cwd: REPOS, reps: 3 });
  const o = ocAll('lspSearch', [{ goal: t.task, reasoning: 'semantic identity vs text', ...t.q }], { reps: 2 });
  let truth = t.truth;
  const rgHits = rgK(s.stdout);
  if (!truth && t.lang === 'c') truth = rgHits.filter(k => !['c/src/zmalloc.c:986', 'c/src/zmalloc.c:1045', 'c/src/db.c:2874'].includes(k)); // the 3 excluded hits are comments (verified by reading)
  const neutral = new Set(t.neutral);
  const strip = (arr) => arr.filter(x => !neutral.has(x));
  const first = o.parts[0]?.results?.[0]?.data || {};
  const rec = { id: t.id, lang: t.lang, task: t.task,
    shell: { cmd: t.shell, calls: 1, chars: s.chars, ms: s.ms, correct: truth ? pr(strip(rgHits), truth) : { hits: rgHits.length, note: 'no LSP truth possible without a server' } },
    octocode: { query: t.q, calls: o.calls, chars: o.chars, ms: o.ms, msAll: o.steps.map(x => x.ms),
      correct: truth ? pr(strip(lspKeys(o, t.scope)), truth) : { error: first.errorCode || first.error || null },
      serverAvailable: first.lsp?.serverAvailable ?? (first.payload ? true : null), coverage: first.payload?.coverage || null, steps: o.steps } };
  out.push(rec);
  const c = (x) => x.precision !== undefined ? `P${x.precision}/R${x.recall} (${x.found}/${x.truth})` : JSON.stringify(x).slice(0, 50);
  console.log(`${t.id.padEnd(12)} rg ${s.chars}c ${s.ms}ms ${c(rec.shell.correct).padEnd(24)} | lsp ${o.calls}x ${o.chars}c ${o.ms}ms ${c(rec.octocode.correct)} ${JSON.stringify(rec.octocode.coverage)}`);
}
for (const t of DEFS) {
  const steps = t.shellSteps.map(c => sh(c, { cwd: REPOS }));
  const o = oc('lspSearch', [{ goal: t.task, reasoning: 'follow the call to its definition', ...t.q }], { reps: 2 });
  const d = o.parsed?.results?.[0]?.data || {};
  const locs = (d.payload?.locations || []).map(L => `${path.relative(REPOS, path.resolve(o.parsed.base || '', L.path || d.path || ''))}:${L.displayRange?.startLine}`);
  const rec = { id: t.id, lang: t.lang, task: t.task,
    shell: { cmd: t.shellSteps.join('  ;  '), calls: steps.length, chars: steps.reduce((a, x) => a + x.chars, 0), ms: steps.reduce((a, x) => a + x.ms, 0),
      correct: { note: 'shell yields candidate list; agent must disambiguate by reading imports/types', outputs: steps.map(x => x.stdout.slice(0, 600)) } },
    octocode: { query: t.q, calls: 1, chars: o.chars, ms: o.ms, correct: { ok: locs.includes(t.truth), got: locs, want: t.truth }, stdout: o.stdout.slice(0, 3000) } };
  out.push(rec);
  console.log(`${t.id.padEnd(12)} shell ${rec.shell.calls}x ${rec.shell.chars}c ${rec.shell.ms}ms | lsp 1x ${o.chars}c ${o.ms}ms ${JSON.stringify(rec.octocode.correct)}`);
}
saveRaw('lsp', out);
