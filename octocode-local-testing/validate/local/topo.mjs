// astTopology dependents vs rg import grep. Truth: independent import parsers (python `ast`, regex+resolver for TS,
// import-block parse for Go, #include parse for C, import + same-package reference for Java).
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { REPOS, sh, ocAll, pr, saveRaw } from './lib.mjs';

const A = (p) => path.join(REPOS, p);
const lsFiles = (repo, spec) => spawnSync('bash', ['-c', `git -c core.quotepath=off -C ${A(repo)} ls-files -- ${spec}`], { encoding: 'utf8', maxBuffer: 1 << 28 }).stdout.split('\n').filter(Boolean).map(f => `${repo}/${f}`);

function truthTS() {
  const dir = 'tsx/packages/element/src'; const target = A(`${dir}/mutateElement.ts`);
  const files = lsFiles('tsx', `'packages/element/src/*.ts' 'packages/element/src/*.tsx'`);
  return files.filter(f => { const s = fs.readFileSync(A(f), 'utf8');
    for (const m of s.matchAll(/(?:from|import)\s*\(?\s*['"]([^'"]+)['"]/g)) { if (!m[1].startsWith('.')) continue;
      const base = path.resolve(path.dirname(A(f)), m[1]); if ([base, base + '.ts', base + '.tsx', path.join(base, 'index.ts')].includes(target)) return true; }
    return false; }).filter(f => A(f) !== target);
}
function truthPY() {
  const py = `
import ast,subprocess,os,sys
root=${JSON.stringify(A('python'))}
files=subprocess.run(['git','-C',root,'ls-files','--','*.py'],capture_output=True,text=True).stdout.split()
T='django.utils.text'
for f in files:
  p=os.path.join(root,f)
  try: tree=ast.parse(open(p,encoding='utf-8').read())
  except Exception: continue
  pkg='.'.join(f[:-3].split('/')[:-1])
  hit=False
  for n in ast.walk(tree):
    if isinstance(n,ast.Import):
      if any(a.name==T or a.name.startswith(T+'.') for a in n.names): hit=True
    elif isinstance(n,ast.ImportFrom):
      mod=n.module or ''
      if n.level:
        parts=pkg.split('.')
        if f.endswith('__init__.py'): parts=f[:-3].split('/')[:-1]
        base=parts[:len(parts)-(n.level-1)] if n.level>1 else parts
        mod='.'.join(base+([mod] if mod else []))
      if mod==T: hit=True
      elif mod=='django.utils' and any(a.name=='text' for a in n.names): hit=True
  if hit and f!='django/utils/text.py': print('python/'+f)
`;
  return spawnSync('python3', ['-c', py], { encoding: 'utf8' }).stdout.split('\n').filter(Boolean);
}
function truthGO() {
  const files = lsFiles('go', `'*.go'`);
  const pkg = 'github.com/prometheus/prometheus/tsdb/chunkenc';
  return files.filter(f => { const s = fs.readFileSync(A(f), 'utf8');
    const blocks = [...s.matchAll(/^import\s*\(([\s\S]*?)^\)/gm)].map(m => m[1]).join('\n') + '\n' + [...s.matchAll(/^import\s+(?:\w+\s+)?("[^"]+")/gm)].map(m => m[1]).join('\n');
    return blocks.includes(`"${pkg}"`); });
}
function truthC() {
  return lsFiles('c', `'src/*.c' 'src/*.h'`).filter(f => /^\s*#\s*include\s+"(?:[^"]*\/)?zmalloc\.h"/m.test(fs.readFileSync(A(f), 'utf8')) && !f.endsWith('/zmalloc.h'));
}
function truthJAVA() {
  const files = lsFiles('java', `'guava/src/*.java'`);
  const samePkg = 'guava/src/com/google/common/collect/';
  return files.filter(f => { if (f.endsWith('/collect/Lists.java')) return false; const s = fs.readFileSync(A(f), 'utf8');
    if (/^import\s+(static\s+)?com\.google\.common\.collect\.Lists(\.\w+|\.\*)?;/m.test(s)) return true;
    if (f.startsWith(`java/${samePkg}`) && !f.slice(`java/${samePkg}`.length).includes('/')) {
      const code = s.replace(/\/\*[\s\S]*?\*\//g, '').replace(/\/\/.*$/gm, '').replace(/"(?:\\.|[^"\\])*"/g, '""');
      return /\bLists\s*\./.test(code); }
    return false; });
}

const TASKS = [
  { id: 'topo-ts', lang: 'ts', root: 'tsx/packages/element/src', file: 'tsx/packages/element/src/mutateElement.ts', truth: truthTS,
    shell: `rg -l -t ts "(from|import)\\s*\\(?\\s*['\\"](\\./|\\.\\./)+mutateElement['\\"]" tsx/packages/element/src` },
  { id: 'topo-python', lang: 'python', root: 'python', file: 'python/django/utils/text.py', truth: truthPY,
    shell: `rg -l -t py -e 'from django\\.utils\\.text import' -e 'import django\\.utils\\.text' -e 'from django\\.utils import .*\\btext\\b' -e 'from \\.text import' python` },
  { id: 'topo-python-subroot', lang: 'python', root: 'python/django', file: 'python/django/utils/text.py', truth: () => truthPY().filter(f => f.startsWith('python/django/')),
    shell: `rg -l -t py -e 'from django\\.utils\\.text import' -e 'import django\\.utils\\.text' -e 'from django\\.utils import .*\\btext\\b' -e 'from \\.text import' python/django` },
  { id: 'topo-go', lang: 'go', root: 'go', file: 'go/tsdb/chunkenc/chunk.go', truth: truthGO,
    shell: `rg -l -F '"github.com/prometheus/prometheus/tsdb/chunkenc"' -t go go` },
  { id: 'topo-go-subroot', lang: 'go', root: 'go/tsdb', file: 'go/tsdb/chunkenc/chunk.go', truth: () => truthGO().filter(f => f.startsWith('go/tsdb/')),
    shell: `rg -l -F '"github.com/prometheus/prometheus/tsdb/chunkenc"' -t go go/tsdb` },
  { id: 'topo-c', lang: 'c', root: 'c/src', file: 'c/src/zmalloc.h', truth: truthC,
    shell: `rg -l '^\\s*#\\s*include\\s+"zmalloc\\.h"' c/src` },
  { id: 'topo-java', lang: 'java', root: 'java/guava/src', file: 'java/guava/src/com/google/common/collect/Lists.java', truth: truthJAVA,
    shell: `rg -l 'import (static )?com\\.google\\.common\\.collect\\.Lists[.;]' java/guava/src; rg -l '\\bLists\\s*\\.' java/guava/src/com/google/common/collect --max-depth 1` },
];
const out = [];
for (const t of TASKS) {
  const truth = [...new Set(t.truth())];
  const s = sh(t.shell, { cwd: REPOS });
  const shellSet = [...new Set(s.stdout.split('\n').filter(Boolean))].filter(f => f !== t.file);
  const q = { analysis: 'dependents', path: A(t.root), file: A(t.file), pageSize: 100 };
  const o = ocAll('astTopology', [q], { reps: 2 });
  const got = [];
  for (const p of o.parts) for (const r of p?.results || []) for (const x of r.data?.results || []) got.push(path.relative(REPOS, path.resolve(p.base || A(t.root), x.file)));
  const d = o.parts[0]?.results?.[0]?.data || {};
  const rec = { id: t.id, lang: t.lang, task: `Direct dependents of ${path.basename(t.file)} (root ${t.root})`,
    shell: { cmd: t.shell, calls: t.shell.includes('; ') ? 2 : 1, chars: s.chars, ms: s.ms, correct: pr(shellSet, truth) },
    octocode: { query: q, calls: o.calls, chars: o.chars, ms: o.ms, correct: pr([...new Set(got)], truth), completeness: d.completeness, imports: d.coverage?.imports, confidence: d.confidence, steps: o.steps } };
  out.push(rec);
  const c = (x) => `P${x.precision}/R${x.recall} (${x.found}/${x.truth})`;
  console.log(`${t.id.padEnd(20)} rg ${rec.shell.calls}x ${s.chars}c ${s.ms}ms ${c(rec.shell.correct).padEnd(22)} | topo ${o.calls}x ${o.chars}c ${o.ms}ms ${c(rec.octocode.correct)} ${JSON.stringify(d.completeness)} miss=${rec.octocode.correct.missing.slice(0, 3)} extra=${rec.octocode.correct.extra.slice(0, 3)} | rg miss=${rec.shell.correct.missing.slice(0, 3)} extra=${rec.shell.correct.extra.slice(0, 3)}`);
}
saveRaw('topo', out);
