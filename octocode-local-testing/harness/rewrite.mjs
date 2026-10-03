// astRewrite (CLI-only, beta): previews on a real corpus never write, both
// rule shapes and inferred langType/ruleKind agree, and a guarded apply runs
// only on a temporary copy.
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { REPOS, RESULTS, ROOT, checks, writeResults } from './mcp-client.mjs';

const { check, summary } = checks('rewrite');
const CLI = path.join(ROOT, 'packages/octocode/out/octocode.js');
const CORPUS = path.join(REPOS, 'rust');
const SCOPE = path.join(CORPUS, 'tokio/src/fs');
const env = { ...process.env, OCTOCODE_BETA: 'true' };

function rewrite(query) {
  const started = Date.now();
  let stdout;
  try {
    stdout = execFileSync(process.execPath, [CLI, 'astRewrite', JSON.stringify({ queries: [query] })], { cwd: ROOT, env, encoding: 'utf8', maxBuffer: 64 << 20 });
  } catch (error) {
    stdout = error.stdout ?? '';
  }
  const bytes = Buffer.byteLength(stdout);
  let data;
  try {
    data = JSON.parse(stdout).results?.[0]?.data;
  } catch {
    data = undefined;
  }
  return { data: data ?? {}, bytes, ms: Date.now() - started, text: stdout };
}

const gitStatus = () => execFileSync('git', ['-C', CORPUS, 'status', '--short'], { encoding: 'utf8' });
const hunkLines = file => {
  const lines = new Set();
  for (const match of (file.patch ?? '').matchAll(/^@@ -(\d+),?(\d+)? /gm)) {
    const start = Number(match[1]);
    const count = match[2] === undefined ? 1 : Number(match[2]);
    for (let line = start; line < start + Math.max(count, 1); line++) lines.add(line);
  }
  return lines;
};

const before = gitStatus();
const measured = {};

// T1: pattern rewrite, ruleKind and langType inferred.
const t1 = rewrite({ path: SCOPE, pattern: '$X.unwrap()', rewrite: '$X.expect("checked")' });
measured.t1 = { bytes: t1.bytes, ms: t1.ms, matches: t1.data.totalMatches };
check('T1 preview infers ruleKind/langType and finds matches', t1.data.mode === 'preview' && t1.data.totalMatches > 0, t1.text.slice(0, 200));
const rows = t1.data.matches ?? [];
check('T1 match rows are lean (16-hex id, path, line)', rows.length > 0 && rows.every(row => /^[a-f0-9]{16}$/.test(row.id) && row.path && Number.isInteger(row.line) && !('text' in row) && !('range' in row)), JSON.stringify(rows[0] ?? {}));
const files = t1.data.files ?? [];
// A complete preview states each file's beforeHash once, in hints.apply.expectedHashes.
const hashOf = file => file.beforeHash ?? t1.data.hints?.apply?.query?.expectedHashes?.[file.path];
check('T1 file rows drop afterHash/patchBytes/absolutePath; every file hash is stated', files.length > 0 && files.every(file => /^[a-f0-9]{64}$/.test(hashOf(file) ?? '') && file.patch && !('afterHash' in file) && !('patchBytes' in file) && !('absolutePath' in file)), JSON.stringify(Object.keys(files[0] ?? {})));
check('T1 every match line lies inside a hunk of its file', rows.every(row => hunkLines(files.find(file => file.path === row.path) ?? {}).has(row.line)), '');
const apply = t1.data.hints?.apply?.query;
check('T1 hints.apply pins langType, snapshot and every file hash', apply?.apply === true && apply.langType === 'rust' && /^[a-f0-9]{64}$/.test(apply.snapshot ?? '') && Object.keys(apply.expectedHashes ?? {}).length === t1.data.affectedFiles, JSON.stringify(apply ?? {}).slice(0, 200));
// Regression guard: baseline 6,581 B; plan A5's 4,000 B would need a
// 2-line patch context (kept at 3, see astRewrite PLAN Results).
check(`T1 preview ≤ 4,400 B (${t1.bytes} B)`, t1.bytes <= 4400, `${t1.bytes} B`);

// The explicit pre-2026-10 shape previews identically.
const explicit = rewrite({ path: SCOPE, langType: 'rust', ruleKind: 'pattern', pattern: '$X.unwrap()', rewrite: '$X.expect("checked")' });
check('explicit langType/ruleKind gives the same snapshot', explicit.data.hints?.apply?.query?.snapshot === apply?.snapshot, '');

// T2: a YAML-string rule and the object rule are one rule.
const yamlRule = 'pattern: $X.unwrap()\ninside:\n  kind: let_declaration\n  stopBy: end';
const objectRule = { pattern: '$X.unwrap()', inside: { kind: 'let_declaration', stopBy: 'end' } };
const t2yaml = rewrite({ path: SCOPE, rule: yamlRule, fix: '$X.expect("checked")' });
const t2obj = rewrite({ path: SCOPE, langType: 'rust', ruleKind: 'rule', rule: objectRule, fix: '$X.expect("checked")' });
measured.t2 = { bytes: t2obj.bytes, ms: t2obj.ms, matches: t2obj.data.totalMatches };
check('T2 YAML-string and object rules preview identically', t2yaml.data.totalMatches > 0 && JSON.stringify(t2yaml.data.files) === JSON.stringify(t2obj.data.files) && t2yaml.data.hints?.apply?.query?.snapshot === t2obj.data.hints?.apply?.query?.snapshot, `${t2yaml.data.totalMatches} vs ${t2obj.data.totalMatches}`);
check(`T2 preview ≤ 4,600 B (${t2obj.bytes} B)`, t2obj.bytes <= 4600, `${t2obj.bytes} B`);

check('previews leave the corpus untouched (git status unchanged)', gitStatus() === before, gitStatus().slice(0, 200));

// Apply only on a temporary copy inside the workspace.
const temp = path.join(RESULTS, `rewrite-apply-${process.pid}`);
fs.rmSync(temp, { recursive: true, force: true });
fs.cpSync(SCOPE, temp, { recursive: true });
try {
  const preview = rewrite({ path: temp, pattern: '$X.unwrap()', rewrite: '$X.expect("checked")' });
  const guarded = preview.data.hints?.apply?.query;
  check('temp copy previews the same matches as the corpus', preview.data.totalMatches === t1.data.totalMatches && !!guarded, '');
  const applied = rewrite(guarded ?? {});
  check('hints.apply commits on the temp copy', applied.data.transaction?.committed === true, applied.text.slice(0, 200));
  const rewritten = fs.readdirSync(temp, { recursive: true }).filter(name => name.endsWith('.rs')).map(name => fs.readFileSync(path.join(temp, name), 'utf8')).join('\n');
  check('applied files carry the replacement', rewritten.includes('.expect("checked")'), '');
  const replay = rewrite(guarded ?? {});
  check('replaying the apply is rejected (snapshot changed)', replay.data.errorCode === 'ast.rewrite.snapshot_changed', replay.data.errorCode ?? '');
  const after = rewrite({ path: temp, pattern: '$X.unwrap()', rewrite: '$X.expect("checked")' });
  check('a fresh preview finds nothing left to rewrite', after.data.totalMatches === 0, String(after.data.totalMatches));
} finally {
  fs.rmSync(temp, { recursive: true, force: true });
}
check('corpus still untouched after the temp apply', gitStatus() === before, '');

writeResults('rewrite', { measured, ...summary() });
