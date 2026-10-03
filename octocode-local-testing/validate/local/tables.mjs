// Renders raw/*.json into markdown tables (raw/tables.md). No numbers are typed by hand.
import fs from 'node:fs';
import path from 'node:path';
import { HERE } from './lib.mjs';

const R = (n) => JSON.parse(fs.readFileSync(path.join(HERE, 'raw', `${n}.json`), 'utf8'));
const esc = (s) => String(s ?? '').replace(/\|/g, '\\|').replace(/\n/g, ' ');
const short = (s, n = 90) => { s = esc(s); return s.length > n ? s.slice(0, n - 1) + '…' : s; };
const corr = (c) => {
  if (!c) return '';
  if (c.precision !== undefined) return `P${c.precision} R${c.recall} (${c.tp ?? ''}/${c.truth})`;
  if (c.exact !== undefined) return c.exact ? `exact (${c.sumTruth} over ${c.filesTruth} files)` : `WRONG ${c.sumGot}/${c.sumTruth}`;
  if (c.orderExact !== undefined) return c.orderExact && c.countsExact ? 'exact order+counts' : `order ${c.orderExact} counts ${c.countsExact}`;
  if (c.ok !== undefined) return c.ok ? 'OK' : `FAIL ${short(JSON.stringify(c), 80)}`;
  if (c.error) return `error: ${c.error}`;
  if (c.hits !== undefined) return `${c.hits} hits (no truth)`;
  if (c.note) return short(c.note, 60);
  return short(JSON.stringify(c), 60);
};
const q = (o) => short(JSON.stringify(Object.fromEntries(Object.entries(o || {}).filter(([k]) => !['goal', 'reasoning', 'path', 'uri', 'file'].includes(k)))), 110);
const verdict = (r) => {
  const s = r.shell, o = r.octocode; const sc = s.correct || {}, oc = o.correct || {};
  const score = (c) => c.precision !== undefined ? (c.precision + c.recall) / 2 : c.exact !== undefined ? +c.exact : c.orderExact !== undefined ? +(c.orderExact && c.countsExact) : c.ok !== undefined ? +c.ok : NaN;
  const a = score(sc), b = score(oc);
  if (!isNaN(a) && !isNaN(b) && Math.abs(a - b) > 0.001) return b > a ? '**octocode** (correctness)' : '**shell** (correctness)';
  if (o.chars === 0) return '—';
  const ratio = o.chars / Math.max(1, s.chars);
  return ratio <= 0.9 ? `tie on correctness; octocode reads ${(1 / ratio).toFixed(1)}× less` : ratio >= 1.1 ? `tie on correctness; shell reads ${ratio.toFixed(1)}× less` : 'tie';
};
let md = '';
const table = (title, rows, opts = {}) => {
  md += `\n### ${title}\n\n| task | expert shell command | shell chars / calls / ms / correct | octocode query | octocode chars / calls / ms / correct | verdict |\n|---|---|---|---|---|---|\n`;
  for (const r of rows) {
    md += `| ${short(r.id + ': ' + r.task, 70)} | \`${short(r.shell.cmd, 95)}\` | ${r.shell.chars} / ${r.shell.calls} / ${r.shell.ms} / ${corr(r.shell.correct)} | \`${q(r.octocode.query)}\` | ${r.octocode.chars} / ${r.octocode.calls} / ${r.octocode.ms} / ${corr(r.octocode.correct)} | ${opts.verdict ? opts.verdict(r) : verdict(r)} |\n`;
  }
  const sum = (k, side) => rows.reduce((a, r) => a + (r[side][k] || 0), 0);
  md += `| **total** | | ${sum('chars', 'shell')} / ${sum('calls', 'shell')} / ${sum('ms', 'shell')} | | ${sum('chars', 'octocode')} / ${sum('calls', 'octocode')} / ${sum('ms', 'octocode')} | |\n`;
};
table('localSearch vs rg', R('search'));
table('localFetch vs sed / rg -C / head / tail', R('fetch'));
table('structureSearch vs git ls-files / find / wc -l', R('structure').filter(r => !r.id.startsWith('tree-find')));
md += '\nPlain `find` baseline for the same trees (no gitignore awareness):\n\n| task | find chars | find correct |\n|---|---|---|\n' + R('structure').filter(r => r.id.startsWith('tree-find')).map(r => `| ${r.id} | ${r.shell.chars} | ${corr(r.shell.correct)} |`).join('\n') + '\n';
const ast = R('ast');
table('astSearch match vs ast-grep CLI', ast.filter(r => r.id.startsWith('match')).map(r => ({ ...r, shell: { ...r.shell, correct: { note: `reference (truth source); ${r.shell.calls === 2 ? 'first bare pattern misparsed → 0 hits, second call = context/selector rule' : 'plain output prints every line of multi-line matches'}` } } })),
  { verdict: (r) => r.octocode.correct.recall < 1 ? '**shell** (octocode misses matches)' : r.shell.calls > 1 ? 'octocode: right on first try (CLI needed a rule)' : `tie on correctness; shell reads ${(r.octocode.chars / r.shell.chars).toFixed(1)}× less` });
md += '\nrg text approximation of the same structural queries:\n\n| task | rg command | chars | correct vs structural truth |\n|---|---|---|---|\n' + ast.filter(r => r.rg).map(r => `| ${r.id} | \`${short(r.rg.cmd, 90)}\` | ${r.rg.chars} | ${corr(r.rg.correct)} |`).join('\n') + '\n';
table('astSearch symbols vs rg declaration regex / ctags', ast.filter(r => r.id.startsWith('symbols')));
const lsp = R('lsp');
table('lspSearch vs rg -w', lsp.filter(r => r.id.startsWith('refs')));
table('lspSearch definition vs rg candidate hunt', lsp.filter(r => r.id.startsWith('def')), { verdict: (r) => r.octocode.correct.ok ? 'octocode resolves exactly; shell returns candidates' : '**shell**' });
table('astTopology dependents vs rg import grep', R('topo'));
md += '\n### astRewrite vs ast-grep --rewrite vs sed\n\n| task | ast-grep (preview+apply) chars / calls / ms / changed lines | sed chars / calls / changed lines / diff vs ast-grep / parse errors after | octocode (preview pages + apply) chars / calls / ms / matches / changed lines / diff vs ast-grep | verdict |\n|---|---|---|---|---|\n';
for (const r of R('rewrite')) md += `| ${short(r.task, 70)} | ${r.shell.chars} / ${r.shell.calls} / ${r.shell.ms} / ${r.shell.changedLines} | ${r.sed.chars} / 1 / ${r.sed.changedLines} / ${r.sed.vsAstGrep.files} files, ${r.sed.vsAstGrep.hunks} hunks / ${r.sed.parseErrorsAfter} (before ${r.parseErrorsBefore}) | ${r.octocode.chars} / ${r.octocode.calls} / ${r.octocode.ms} / ${r.octocode.previewMatches} / ${r.octocode.changedLines} / ${r.octocode.vsAstGrep.files} files, ${r.octocode.vsAstGrep.hunks} hunks | ${r.octocode.vsAstGrep.hunks === 0 ? 'octocode = ast-grep result' : '**differs**'}; sed ${r.sed.vsAstGrep.hunks ? 'wrong' : 'same'} |\n`;
md += '\n### Edge cases\n\n| id | scenario | shell (cmd → chars, exit) | octocode (chars, exit) | check |\n|---|---|---|---|---|\n';
for (const r of R('edge')) md += `| ${r.id} | ${short(r.title, 70)} | \`${short(r.shell.cmd, 70)}\` → ${r.shell.chars}c, exit ${r.shell.code ?? ''} | ${r.octocode.chars}c, exit ${r.octocode.code ?? ''} | ${short(JSON.stringify(r.check), 160)} |\n`;
fs.writeFileSync(path.join(HERE, 'raw', 'tables.md'), md);
console.log(md.length, 'chars written to raw/tables.md');
