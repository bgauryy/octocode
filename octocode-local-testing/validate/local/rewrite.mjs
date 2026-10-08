// astRewrite (preview → next.apply replayed verbatim) vs ast-grep --rewrite (preview → -U) vs sed.
// Every run works on fresh copies under fixtures/rewrite/<id>/{oc,sg,sed}; product repos are never touched.
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { HERE, REPOS, sh, oc, ocAll, ocRaw, saveRaw } from './lib.mjs';

const FX = path.join(HERE, 'fixtures/rewrite');
const cp = (src, dst) => { fs.rmSync(dst, { recursive: true, force: true }); fs.mkdirSync(path.dirname(dst), { recursive: true }); spawnSync('cp', ['-R', src, dst]); };
const errNodes = (dir, lang) => spawnSync('ast-grep', ['scan', '--inline-rules', `id: e\nlanguage: ${lang}\nrule:\n  kind: ERROR`, dir, '--json=stream'], { encoding: 'utf8', maxBuffer: 1 << 28 }).stdout.split('\n').filter(Boolean).length;
const diffStat = (a, b) => { const r = spawnSync('diff', ['-r', '-U0', a, b], { encoding: 'utf8', maxBuffer: 1 << 28 }).stdout; return { files: (r.match(/^diff -r/mg) || []).length, hunks: (r.match(/^@@/mg) || []).length, sample: r.slice(0, 1500) }; };
const changedLines = (orig, dir) => { const r = spawnSync('diff', ['-r', '-U0', orig, dir], { encoding: 'utf8', maxBuffer: 1 << 28 }).stdout; return (r.match(/^\+[^+]/mg) || []).length; };

const TASKS = [
  { id: 'rw-rust', lang: 'rust', src: 'rust/tokio/src/sync', pattern: '$X.unwrap()', rewrite: '$X.expect("checked")',
    sed: `rg -l -F '.unwrap()' -t rust DIR | xargs sed -i '' 's/\\.unwrap()/.expect("checked")/g'` },
  { id: 'rw-ts', lang: 'typescript', src: 'tsx/packages/element/src', pattern: 'new Error($MSG)', rewrite: 'new AppError($MSG)',
    sed: `rg -l -F 'new Error(' DIR | xargs sed -i '' 's/new Error(/new AppError(/g'` },
  { id: 'rw-ts-variadic', lang: 'typescript', src: 'tsx/packages/element/src', pattern: 'new Error($$$A)', rewrite: 'new AppError($$$A)',
    sed: `rg -l -F 'new Error(' DIR | xargs sed -i '' 's/new Error(/new AppError(/g'` },
  { id: 'rw-go', lang: 'go', src: 'go/tsdb/chunkenc', pattern: 'require.NoError(t, $E)', rewrite: 'require.NoError(t, $E, "chunkenc")',
    sed: `rg -l -F 'require.NoError(t, ' DIR | xargs sed -i '' -E 's/require\\.NoError\\(t, ([^()]*(\\([^()]*\\))?[^()]*)\\)/require.NoError(t, \\1, "chunkenc")/g'` },
  { id: 'rw-python', lang: 'python', src: 'python/tests/migrations', pattern: 'self.assertEqual(len($A), 0)', rewrite: 'self.assertLen($A, 0)',
    sed: `rg -l -F 'self.assertEqual(len(' DIR | xargs sed -i '' -E 's/self\\.assertEqual\\(len\\((.*)\\), 0\\)/self.assertLen(\\1, 0)/g'` },
];

const out = [];
for (const t of TASKS) {
  const base = path.join(FX, t.id); const orig = path.join(base, 'orig');
  cp(path.join(REPOS, t.src), orig);
  for (const k of ['oc', 'sg', 'sed']) cp(orig, path.join(base, k));
  const dOC = path.join(base, 'oc'), dSG = path.join(base, 'sg'), dSED = path.join(base, 'sed');
  // ---- ast-grep: preview then apply (-U)
  const sgPrev = sh(`ast-grep run -p '${t.pattern}' -r '${t.rewrite}' -l ${t.lang} ${dSG}`, { cwd: HERE, reps: 1 });
  const sgApply = sh(`ast-grep run -p '${t.pattern}' -r '${t.rewrite}' -l ${t.lang} -U ${dSG}`, { cwd: HERE, reps: 1 });
  // ---- sed: no preview possible; single blind pass
  const sedRun = sh(t.sed.replace('DIR', dSED), { cwd: HERE, reps: 1 });
  // ---- octocode: preview then replay next.apply verbatim
  const q = { goal: `codemod ${t.pattern} -> ${t.rewrite}`, reasoning: 'intended structural edit', path: dOC, langType: t.lang, ruleKind: 'pattern', pattern: t.pattern, rewrite: t.rewrite };
  q.pageSize = 1000; // expert: fewest preview pages
  const prevAll = ocAll('astRewrite', [q], { cwd: HERE, reps: 1 });
  const prev = { chars: prevAll.chars, ms: prevAll.ms, calls: prevAll.calls, stdout: prevAll.steps.map(x => x.stdout).join('\n') };
  const datas = prevAll.parts.map(p => p?.results?.[0]?.data || {});
  const d = { ...datas[0], ...datas[datas.length - 1], matchCount: datas[0].matchCount, next: datas.map(x => x.next).filter(Boolean).find(n => n.apply) || datas[datas.length - 1].next };
  let apply = null, applyData = null;
  if (d.hints?.apply?.query) { apply = ocRaw('astRewrite', { queries: [d.hints.apply.query] }, { cwd: HERE, reps: 1 }); applyData = apply.parsed?.results?.[0]?.data; }
  const rec = { id: t.id, lang: t.lang, task: `Rewrite ${t.pattern} → ${t.rewrite} in a copy of ${t.src}`,
    shell: { cmd: `ast-grep run -p … -r … (preview)  ;  ast-grep … -U`, calls: 2, chars: sgPrev.chars + sgApply.chars, ms: sgPrev.ms + sgApply.ms, changedLines: changedLines(orig, dSG), parseErrorsAfter: errNodes(dSG, t.lang) },
    sed: { cmd: t.sed, calls: 1, chars: sedRun.chars, ms: sedRun.ms, changedLines: changedLines(orig, dSED), parseErrorsAfter: errNodes(dSED, t.lang), vsAstGrep: diffStat(dSG, dSED) },
    octocode: { query: q, calls: prev.calls + (apply ? 1 : 0), previewCalls: prev.calls, chars: prev.chars + (apply?.chars || 0), ms: prev.ms + (apply?.ms || 0),
      previewMatches: d.matchCount, previewComplete: d.complete, affectedFiles: d.affectedFiles, previewError: d.error || null,
      applyResult: applyData ? Object.fromEntries(Object.entries(applyData).filter(([k]) => !['files', 'matches'].includes(k))) : null,
      changedLines: changedLines(orig, dOC), parseErrorsAfter: errNodes(dOC, t.lang), vsAstGrep: diffStat(dSG, dOC), previewStdout: prev.stdout.slice(0, 4000), applyStdout: apply?.stdout.slice(0, 3000) },
    parseErrorsBefore: errNodes(orig, t.lang) };
  out.push(rec);
  console.log(`${t.id.padEnd(15)} sg 2x ${rec.shell.chars}c ${rec.shell.ms}ms Δ${rec.shell.changedLines} err${rec.parseErrorsBefore}->${rec.shell.parseErrorsAfter} | sed 1x ${rec.sed.chars}c Δ${rec.sed.changedLines} err->${rec.sed.parseErrorsAfter} vsSG ${rec.sed.vsAstGrep.files}f/${rec.sed.vsAstGrep.hunks}h | oc ${rec.octocode.calls}x ${rec.octocode.chars}c ${rec.octocode.ms}ms matches=${rec.octocode.previewMatches} complete=${rec.octocode.previewComplete} Δ${rec.octocode.changedLines} err->${rec.octocode.parseErrorsAfter} vsSG ${rec.octocode.vsAstGrep.files}f/${rec.octocode.vsAstGrep.hunks}h ${rec.octocode.previewError || ''}`);
}
saveRaw('rewrite', out);
