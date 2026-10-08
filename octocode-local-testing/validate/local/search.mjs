// localSearch vs expert rg. Ground truth = independent engine (git grep / python re over git ls-files).
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { REPOS, sh, ocAll, pr, saveRaw } from './lib.mjs';

const A = (p) => path.join(REPOS, p);
const run = (cmd) => spawnSync('bash', ['-c', cmd], { cwd: REPOS, encoding: 'utf8', maxBuffer: 1 << 29 }).stdout;

// keys relative to REPOS
function rgLineKeys(out) {
  const k = []; for (const l of out.split('\n')) { const m = l.match(/^(.+?)[:-](\d+)[:-]/); if (m && /:\d+:/.test(l)) k.push(`${m[1]}:${m[2]}`); } return k;
}
function rgFiles(out) { return out.split('\n').filter(Boolean); }
function rgCounts(out) { const m = {}; for (const l of out.split('\n')) { const x = l.match(/^(.+):(\d+)$/); if (x) m[x[1]] = +x[2]; } return m; }
function ocRows(all) {
  const rows = [];
  for (const p of all.parts) { if (!p) continue; const base = p.base || '';
    for (const r of p.results || []) for (const f of r.data?.files || []) rows.push({ ...f, rel: path.relative(REPOS, path.resolve(base, f.path)) }); }
  return rows;
}
const ocLineKeys = (all) => [...new Set(ocRows(all).flatMap(f => (f.matches || []).flatMap(m => (m.matchedLines || [m.line]).map(l => `${f.rel}:${l}`))))];
const ocFiles = (all) => [...new Set(ocRows(all).map(f => f.rel))];
function ocCounts(all, field) { const m = {}; for (const f of ocRows(all)) m[f.rel] = f[field] ?? f.matchedLineCount ?? f.matchCount ?? f.count; return m; }
function cmpCounts(a, t) {
  const keys = new Set([...Object.keys(a), ...Object.keys(t)]); let diff = [];
  for (const k of keys) if (a[k] !== t[k]) diff.push(`${k}: got ${a[k]} want ${t[k]}`);
  const sa = Object.values(a).reduce((x, y) => x + (y || 0), 0), st = Object.values(t).reduce((x, y) => x + y, 0);
  return { filesGot: Object.keys(a).length, filesTruth: Object.keys(t).length, sumGot: sa, sumTruth: st, exact: diff.length === 0, diff: diff.slice(0, 10) };
}
// independent multiline truth: python re over tracked files
function pyMulti(repoRel, glob, pattern, flags = 'M') {
  const py = `
import re,subprocess,sys,os
root=os.path.join(${JSON.stringify(REPOS)},${JSON.stringify(repoRel)})
top=subprocess.run(['git','-C',root,'rev-parse','--show-toplevel'],capture_output=True,text=True).stdout.strip()
files=subprocess.run(['git','-C',root,'ls-files','--',${JSON.stringify(glob)}],capture_output=True,text=True).stdout.split()
rx=re.compile(${JSON.stringify(pattern)}, re.${flags})
for f in files:
  p=os.path.join(root,f)
  try: s=open(p,encoding='utf-8',errors='replace').read()
  except Exception: continue
  for m in rx.finditer(s):
    print(os.path.relpath(p,${JSON.stringify(REPOS)})+':'+str(s.count('\\n',0,m.start())+1))
`;
  return spawnSync('python3', ['-c', py], { encoding: 'utf8', maxBuffer: 1 << 28 }).stdout.split('\n').filter(Boolean);
}
// git grep truth: returns keys relative to REPOS
function gg(repo, args, pathspec = '') {
  const out = run(`git -C ${repo} grep -n --no-color ${args} ${pathspec ? '-- ' + pathspec : ''}`);
  return out.split('\n').filter(Boolean).map(l => { const m = l.match(/^(.+?):(\d+):/); return m ? `${repo}/${m[1]}:${m[2]}` : null; }).filter(Boolean);
}
function ggFiles(repo, args, pathspec = '') {
  return run(`git -C ${repo} grep -l --no-color ${args} ${pathspec ? '-- ' + pathspec : ''}`).split('\n').filter(Boolean).map(f => `${repo}/${f}`);
}
function ggCounts(repo, args, pathspec = '') {
  const m = {}; for (const l of run(`git -C ${repo} grep -c --no-color ${args} ${pathspec ? '-- ' + pathspec : ''}`).split('\n')) { const x = l.match(/^(.+):(\d+)$/); if (x) m[`${repo}/${x[1]}`] = +x[2]; } return m;
}

const tasks = [
  // ---------------- TypeScript (excalidraw) ----------------
  { id: 'ts-decl-regex', lang: 'ts', task: 'Find declaration(s) of newElement (const|function) in TS',
    shell: `rg -n -t ts '(const|function) newElement\\b' tsx`,
    oc: { path: A('tsx'), searchText: '(const|function) newElement\\b', langType: 'ts', maxMatchesPerFile: 50, pageSize: 100 },
    truth: () => gg('tsx', `-P '(const|function) newElement\\b'`, `'*.ts' '*.tsx' '*.mts' '*.cts'`),
    judge: 'lines', parseShell: rgLineKeys },
  { id: 'ts-imports-l-F-glob', lang: 'ts', task: 'Files importing from "@excalidraw/common", excluding tests',
    shell: `rg -l -t ts -F 'from "@excalidraw/common"' -g '!**/*.test.*' -g '!**/tests/**' tsx`,
    oc: { path: A('tsx'), searchText: 'from "@excalidraw/common"', regex: 'literal', langType: 'ts', exclude: ['**/*.test.*', '**/tests/**'], resultView: 'files', pageSize: 1000 },
    truth: () => ggFiles('tsx', `-F 'from "@excalidraw/common"'`, `'*.ts' '*.tsx' '*.mts' '*.cts' ':!*.test.*' ':!**/tests/**'`),
    judge: 'files', parseShell: rgFiles },
  { id: 'ts-context-C2', lang: 'ts', task: 'Show `throw new Error(` sites with 2 lines context in element/src/transform.ts',
    shell: `rg -n -C2 -F 'throw new Error(' tsx/packages/element/src/transform.ts`,
    oc: { path: A('tsx/packages/element/src/transform.ts'), searchText: 'throw new Error(', regex: 'literal', contextLines: 2, maxMatchesPerFile: 100 },
    truth: () => gg('tsx', `-F 'throw new Error('`, 'packages/element/src/transform.ts'),
    judge: 'lines', parseShell: (o) => o.split('\n').map(l => l.match(/^(\d+):/)).filter(Boolean).map(m => `tsx/packages/element/src/transform.ts:${m[1]}`) },
  // ---------------- Rust (tokio) ----------------
  { id: 'rs-count-w', lang: 'rust', task: 'Per-file count of lines with word `unsafe` in tokio/src',
    shell: `rg -c -w unsafe -t rust rust/tokio/src`,
    oc: { path: A('rust/tokio/src'), searchText: 'unsafe', wholeWord: true, langType: 'rust', resultView: 'countLines', pageSize: 1000 },
    truth: () => ggCounts('rust', `-w -F unsafe`, `'tokio/src/*.rs'`),
    judge: 'counts', parseShell: rgCounts, countField: 'matchedLineCount' },
  { id: 'rs-multiline-U', lang: 'rust', task: '#[track_caller] immediately followed by `pub fn spawn*` (multiline)',
    shell: `rg -U -n '#\\[track_caller\\]\\s*\\n\\s*pub fn spawn' -t rust rust`,
    oc: { path: A('rust'), searchText: '#\\[track_caller\\]\\s*\\n\\s*pub fn spawn', multiline: 'on', langType: 'rust', maxMatchesPerFile: 100, pageSize: 100 },
    truth: () => pyMulti('rust', '*.rs', '#\\[track_caller\\]\\s*\\n\\s*pub fn spawn'),
    // rg -U -n prints every line of a multi-line match; the match START is the #[track_caller] line
    judge: 'lines', parseShell: (o) => o.split('\n').filter(l => /#\[track_caller\]/.test(l)).map(l => l.match(/^(.+?):(\d+):/)).filter(Boolean).map(m => `${m[1]}:${m[2]}`) },
  { id: 'rs-smartcase-S', lang: 'rust', task: 'Smart-case: lines mentioning semaphore (any case) in tokio/src/sync, per-file counts',
    shell: `rg -S -c 'semaphore' rust/tokio/src/sync`,
    oc: { path: A('rust/tokio/src/sync'), searchText: 'semaphore', caseMode: 'smart', resultView: 'countLines', pageSize: 1000 },
    truth: () => ggCounts('rust', `-i -F semaphore`, `'tokio/src/sync/*'`),
    judge: 'counts', parseShell: rgCounts, countField: 'matchedLineCount' },
  // ---------------- Go (prometheus) ----------------
  { id: 'go-l-F-notest', lang: 'go', task: 'Non-test files in tsdb/ that call errors.New("…")',
    shell: `rg -l -F 'errors.New("' -g '!*_test.go' go/tsdb`,
    oc: { path: A('go/tsdb'), searchText: 'errors.New("', regex: 'literal', exclude: ['*_test.go'], resultView: 'files', pageSize: 1000 },
    truth: () => ggFiles('go', `-F 'errors.New("'`, `'tsdb/*' ':!*_test.go'`),
    judge: 'files', parseShell: rgFiles },
  { id: 'go-max-count', lang: 'go', task: 'First Head method in tsdb/head.go (--max-count 1)',
    shell: `rg -n -m1 'func \\(h \\*Head\\) ' go/tsdb/head.go`,
    oc: { path: A('go/tsdb/head.go'), searchText: 'func \\(h \\*Head\\) ', maxMatchesPerFile: 1 }, maxCalls: 1,
    truth: () => gg('go', `-E 'func \\(h \\*Head\\) '`, 'tsdb/head.go').slice(0, 1),
    judge: 'lines', parseShell: (o) => o.split('\n').map(l => l.match(/^(\d+):/)).filter(Boolean).map(m => `go/tsdb/head.go:${m[1]}`) },
  { id: 'go-w-t-all', lang: 'go', task: 'All lines using identifier NewHead (-w, Go files)',
    shell: `rg -n -w NewHead -t go go`,
    oc: { path: A('go'), searchText: 'NewHead', wholeWord: true, langType: 'go', maxMatchesPerFile: 200, pageSize: 200 },
    truth: () => gg('go', `-w -F NewHead`, `'*.go'`),
    judge: 'lines', parseShell: rgLineKeys },
  // ---------------- Python (django) ----------------
  { id: 'py-w-scope', lang: 'python', task: 'Uses of get_object_or_404 in django/ package (exclude tests dir)',
    shell: `rg -n -w get_object_or_404 -t py python/django`,
    oc: { path: A('python/django'), searchText: 'get_object_or_404', wholeWord: true, langType: 'py', maxMatchesPerFile: 100, pageSize: 100 },
    truth: () => gg('python', `-w -F get_object_or_404`, `'django/*.py'`),
    judge: 'lines', parseShell: rgLineKeys },
  { id: 'py-multiline-property', lang: 'python', task: '@property followed by def (multiline) in models/fields/__init__.py',
    shell: `rg -U -n '@property\\s*\\n\\s*def \\w+' python/django/db/models/fields/__init__.py`,
    oc: { path: A('python/django/db/models/fields/__init__.py'), searchText: '@property\\s*\\n\\s*def \\w+', multiline: 'on', maxMatchesPerFile: 200 },
    truth: () => pyMulti('python', 'django/db/models/fields/__init__.py', '@property\\s*\\n\\s*def \\w+'),
    judge: 'lines', parseShell: (o) => o.split('\n').filter(l => /@property/.test(l)).map(l => l.match(/^(\d+):/)).filter(Boolean).map(m => `python/django/db/models/fields/__init__.py:${m[1]}`) },
  // ---------------- Java (guava) ----------------
  { id: 'java-count-total', lang: 'java', task: 'Total lines annotated @CanIgnoreReturnValue in guava/src (count)',
    shell: `rg -c -F '@CanIgnoreReturnValue' -t java java/guava/src`,
    oc: { path: A('java/guava/src'), searchText: '@CanIgnoreReturnValue', regex: 'literal', langType: 'java', resultView: 'countLines', pageSize: 1000 },
    truth: () => ggCounts('java', `-F '@CanIgnoreReturnValue'`, `'guava/src/*.java'`),
    judge: 'counts', parseShell: rgCounts, countField: 'matchedLineCount' },
  { id: 'java-l-F-meta', lang: 'java', task: 'Files in common/base calling checkNotNull( (literal with regex metachar)',
    shell: `rg -l -F 'checkNotNull(' java/guava/src/com/google/common/base`,
    oc: { path: A('java/guava/src/com/google/common/base'), searchText: 'checkNotNull(', regex: 'literal', resultView: 'files', pageSize: 1000 },
    truth: () => ggFiles('java', `-F 'checkNotNull('`, `'guava/src/com/google/common/base/*'`),
    judge: 'files', parseShell: rgFiles },
  // ---------------- C (redis) ----------------
  { id: 'c-def-anchor', lang: 'c', task: 'Definition of zmalloc (anchored ^ regex) in src/',
    shell: `rg -n '^void \\*zmalloc\\(' c/src`,
    oc: { path: A('c/src'), searchText: '^void \\*zmalloc\\(' },
    truth: () => gg('c', `-E '^void \\*zmalloc\\('`, `'src/*'`),
    judge: 'lines', parseShell: rgLineKeys },
  { id: 'c-count-w', lang: 'c', task: 'Per-file count of lines using zfree (word) in src/*.c',
    shell: `rg -c -w zfree -g '*.c' c/src`,
    oc: { path: A('c/src'), searchText: 'zfree', wholeWord: true, include: ['*.c'], resultView: 'countLines', pageSize: 1000 },
    truth: () => ggCounts('c', `-w -F zfree`, `'src/*.c'`),
    judge: 'counts', parseShell: rgCounts, countField: 'matchedLineCount' },
  // ---------------- C++ (nlohmann json) ----------------
  { id: 'cpp-hot-file-all', lang: 'cpp', task: 'All JSON_HEDLEY_ lines under include/ (1 file has 1266 hits: pagination honesty)',
    shell: `rg -n -F JSON_HEDLEY_ cpp/include`,
    oc: { path: A('cpp/include'), searchText: 'JSON_HEDLEY_', regex: 'literal', maxMatchesPerFile: 100000, pageSize: 100 },
    truth: () => gg('cpp', `-F JSON_HEDLEY_`, `'include/*'`),
    judge: 'lines', parseShell: rgLineKeys },
  { id: 'cpp-count-only', lang: 'cpp', task: 'How many lines contain JSON_HEDLEY_ per file under include/ (count only)',
    shell: `rg -c -F JSON_HEDLEY_ cpp/include`,
    oc: { path: A('cpp/include'), searchText: 'JSON_HEDLEY_', regex: 'literal', resultView: 'countLines', pageSize: 1000 },
    truth: () => ggCounts('cpp', `-F JSON_HEDLEY_`, `'include/*'`),
    judge: 'counts', parseShell: rgCounts, countField: 'matchedLineCount' },
];

const only = process.argv[2];
const out = [];
for (const t of tasks) {
  if (only && !t.id.startsWith(only)) continue;
  const s = sh(t.shell, { cwd: REPOS });
  const o = ocAll('localSearch', [{ goal: t.task, reasoning: 'expert query for head-to-head', ...t.oc }], { maxCalls: t.maxCalls });
  const truth = t.truth();
  let shellCorrect, ocCorrect;
  if (t.judge === 'lines') { shellCorrect = pr(t.parseShell(s.stdout), truth); ocCorrect = pr(ocLineKeys(o), truth); }
  else if (t.judge === 'files') { shellCorrect = pr(t.parseShell(s.stdout), truth); ocCorrect = pr(ocFiles(o), truth); }
  else { const T = truth; shellCorrect = cmpCounts(t.parseShell(s.stdout), T); ocCorrect = cmpCounts(ocCounts(o, t.countField), T); }
  const rec = { id: t.id, lang: t.lang, task: t.task,
    shell: { cmd: t.shell, calls: 1, chars: s.chars, ms: s.ms, msAll: s.msAll, code: s.code, correct: shellCorrect },
    octocode: { query: t.oc, calls: o.calls, chars: o.chars, ms: o.ms, truncatedByCap: o.truncatedByCap, correct: ocCorrect, steps: o.steps },
    truthSize: Array.isArray(truth) ? truth.length : Object.keys(truth).length };
  out.push(rec);
  const c = (x) => x.precision !== undefined ? `P${x.precision}/R${x.recall}` : (x.exact ? 'exact' : `diff(${x.sumGot}/${x.sumTruth})`);
  console.log(`${t.id.padEnd(24)} shell ${String(s.chars).padStart(7)}c ${String(s.ms).padStart(5)}ms ${c(shellCorrect).padEnd(14)} | oc ${o.calls}x ${String(o.chars).padStart(7)}c ${String(o.ms).padStart(5)}ms ${c(ocCorrect)}`);
}
saveRaw(only ? `search-${only}` : 'search', out);
