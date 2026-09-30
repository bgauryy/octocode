// structureSearch vs find / git ls-files / wc -l. Truth computed from git ls-files + fs reads in node.
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { REPOS, sh, ocAll, pr, saveRaw } from './lib.mjs';

const A = (p) => path.join(REPOS, p);
const lsFiles = (repo, spec = '') => spawnSync('bash', ['-c', `git -c core.quotepath=off -C ${A(repo)} ls-files ${spec}`], { encoding: 'utf8', maxBuffer: 1 << 28 }).stdout.split('\n').filter(Boolean);

function ocEntries(all) {
  const out = [];
  for (const p of all.parts) { if (!p) continue;
    for (const r of p.results || []) { const d = r.data || {};
      for (const e of d.entries || []) out.push(e.replace(/ \([^)]*\)$/, '').replace(/\/$/, ''));
      for (const f of d.files || []) out.push({ abs: path.resolve(p.base || '', f.path), ...f });
    } }
  return out;
}

const out = [];
const log = (r) => console.log(`${r.id.padEnd(20)} shell ${r.shell.calls}x ${String(r.shell.chars).padStart(6)}c ${String(r.shell.ms).padStart(4)}ms ${JSON.stringify(r.shell.correct).slice(0, 70).padEnd(70)} | oc ${r.octocode.calls}x ${String(r.octocode.chars).padStart(6)}c ${String(r.octocode.ms).padStart(5)}ms ${JSON.stringify(r.octocode.correct).slice(0, 120)}`);
function push(id, lang, task, s, sc, q, o, occ) {
  const r = { id, lang, task, shell: { cmd: s.cmd, calls: 1, chars: s.chars, ms: s.ms, correct: sc },
    octocode: { query: q, calls: o.calls, chars: o.chars, ms: o.ms, correct: occ, steps: o.steps } };
  out.push(r); log(r);
}
const brief = (x) => ({ precision: x.precision, recall: x.recall, found: x.found, truth: x.truth, missing: x.missing.slice(0, 5), extra: x.extra.slice(0, 5) });

// 1) tree depth 2 (two levels) of the repo: truth = tracked files/dirs at depth<=2
const TREES = [['ts', 'tsx'], ['rust', 'rust'], ['go', 'go'], ['python', 'python'], ['java', 'java'], ['c', 'c']];
for (const [lang, repo] of TREES) {
  const tracked = lsFiles(repo);
  const truth = new Set();
  for (const f of tracked) { const parts = f.split('/'); for (let i = 1; i <= Math.min(2, parts.length); i++) truth.add(parts.slice(0, i).join('/')); }
  const cmd = `cd ${repo} && git -c core.quotepath=off ls-files | awk -F/ '{print $1; if (NF>1) print $1"/"$2}' | sort -u`;
  const s = sh(cmd, { cwd: REPOS });
  const q = { operation: 'tree', path: A(repo), maxDepth: 1, hidden: true, pageSize: 100 };
  const o = ocAll('structureSearch', [q]);
  const got = ocEntries(o).filter(x => typeof x === 'string');
  push(`tree-${lang}`, lang, `Two-level layout of ${repo}`, s, brief(pr(s.stdout.split('\n').filter(Boolean), [...truth])), q, o, brief(pr(got, [...truth])));
  // also an untrained "find" user: count what find prints (includes ignored build dirs)
  const s2 = sh(`find ${repo} -mindepth 1 -maxdepth 2 -not -path '*/.git' -not -path '*/.git/*' | sed 's|^${repo}/||' | sort`, { cwd: REPOS });
  push(`tree-find-${lang}`, lang, `Two-level layout of ${repo} (find baseline)`, s2, brief(pr(s2.stdout.split('\n').filter(Boolean), [...truth])), q, { calls: 0, chars: 0, ms: 0, steps: [] }, { note: 'same octocode run as tree-' + lang });
}

// 2) find files by name glob
const NAMES = [
  ['ts', 'tsx/packages', '*.test.tsx'], ['rust', 'rust/tokio/tests', 'sync_*.rs'], ['go', 'go/tsdb', '*_test.go'],
  ['python', 'python/django/contrib', 'models.py'], ['java', 'java/guava/src', '*Builder.java'], ['cpp', 'cpp/tests/src', 'unit-*.cpp'],
];
for (const [lang, dir, glob] of NAMES) {
  const repo = dir.split('/')[0]; const sub = dir.split('/').slice(1).join('/');
  const truth = lsFiles(repo, `-- '${sub}/**/${glob}' '${sub}/${glob}'`).map(f => `${repo}/${f}`);
  const cmd = `find ${dir} -name '${glob}' -type f`;
  const s = sh(cmd, { cwd: REPOS });
  const q = { operation: 'files', path: A(dir), names: [glob], entryType: 'f', pageSize: 100, sort: 'path' };
  const o = ocAll('structureSearch', [q]);
  const got = ocEntries(o).filter(x => typeof x === 'object').map(x => path.relative(REPOS, x.abs));
  push(`names-${lang}`, lang, `Files named ${glob} under ${dir}`, s, brief(pr(s.stdout.split('\n').filter(Boolean), [...new Set(truth)])), q, o, brief(pr(got, [...new Set(truth)])));
}

// 3) ten largest source files by line count
const BIG = [['ts', 'tsx/packages', 'ts'], ['rust', 'rust/tokio/src', 'rs'], ['go', 'go/tsdb', 'go'], ['python', 'python/django', 'py'], ['java', 'java/guava/src', 'java'], ['c', 'c/src', 'c']];
for (const [lang, dir, ext] of BIG) {
  const repo = dir.split('/')[0]; const sub = dir.split('/').slice(1).join('/');
  const files = lsFiles(repo, `-- '${sub}/**/*.${ext}' '${sub}/*.${ext}'`);
  const counted = files.map(f => { const b = fs.readFileSync(path.join(A(repo), f)); let n = 0; for (const c of b) if (c === 10) n++; if (b.length && b[b.length - 1] !== 10) n++; return [`${repo}/${f}`, n]; }).sort((a, b) => b[1] - a[1]);
  const truth = counted.slice(0, 10);
  const cmd = `cd ${repo} && git ls-files -z -- '${sub}/**/*.${ext}' '${sub}/*.${ext}' | xargs -0 wc -l | sort -rn | sed -n '2,11p'`;
  const s = sh(cmd, { cwd: REPOS });
  const shellTop = s.stdout.split('\n').filter(Boolean).map(l => { const m = l.trim().match(/^(\d+) (.+)$/); return m ? [`${repo}/${m[2]}`, +m[1]] : null; }).filter(Boolean);
  const q = { operation: 'files', path: A(dir), extensions: [ext], entryType: 'f', detail: 'full', sort: 'lines', pageSize: 10 };
  const o = ocAll('structureSearch', [q], { maxCalls: 1 });
  const ocTop = ocEntries(o).filter(x => typeof x === 'object').map(x => [path.relative(REPOS, x.abs), x.lineCount]);
  const score = (got) => ({ orderExact: JSON.stringify(got.map(x => x[0])) === JSON.stringify(truth.map(x => x[0])), countsExact: got.every((x, i) => truth[i] && truth[i][1] === x[1]), setP: pr(got.map(x => x[0]), truth.map(x => x[0])).precision, got: got.slice(0, 3), want: truth.slice(0, 3) });
  push(`largest-${lang}`, lang, `10 largest .${ext} files by lines under ${dir}`, s, score(shellTop), q, o, score(ocTop));
}
saveRaw('structure', out);
