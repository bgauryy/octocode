#!/usr/bin/env node
// finalize.mjs — Aggregate q{N}/output.md files into <run>/output.md.
// Usage: node finalize.mjs <run_dir>
import { readFileSync, writeFileSync, existsSync, readdirSync } from 'fs';
import { join, basename } from 'path';

const run = process.argv[2];
if (!run || !existsSync(run)) { console.error('Usage: finalize.mjs <run_dir>'); process.exit(1); }

const num = (s) => parseInt(String(s).replace(/[, ]/g, ''), 10) || 0;
const cell = (text, k) => {
  const m = text.match(new RegExp(`\\|\\s*${k}\\s*\\|\\s*([^|\\n]+?)\\s*\\|`));
  return m ? m[1].trim() : '';
};

const rows = readdirSync(run).filter(d => /^q\d+$/.test(d))
  .sort((a, b) => +a.slice(1) - +b.slice(1))
  .map(d => {
    const q = +d.slice(1);
    const p = join(run, d, 'output.md');
    if (!existsSync(p)) return { q, missing: true };
    const t = readFileSync(p, 'utf8');
    return {
      q,
      calls: num(cell(t, 'Calls')),
      in:    num(cell(t, 'In Chars')),
      out:   num(cell(t, 'Out Chars')),
      ms:    num(cell(t, 'Elapsed ms')),
      one:   (t.split(/## Answer\s*\n+/)[1] || '').split('\n').find(l => l.trim()) || '',
    };
  });

const sum = (k) => rows.reduce((s, r) => s + (r[k] || 0), 0);
const tot = { calls: sum('calls'), in: sum('in'), out: sum('out'), ms: sum('ms') };
const trunc = (s, n = 60) => s.length > n ? s.slice(0, n - 1) + '…' : s;
const slug = basename(run);
const agent = slug.split('-').pop();
const fmt = n => n.toLocaleString('en');

const body = `# Run ${slug}

| Agent | Questions | Calls | In Chars | Out Chars | Elapsed ms |
|-------|----------:|------:|---------:|----------:|-----------:|
| ${agent} | ${rows.filter(r => !r.missing).length} / 31 | ${tot.calls} | ${fmt(tot.in)} | ${fmt(tot.out)} | ${fmt(tot.ms)} |

| Q | Calls | In Chars | Out Chars | Elapsed ms | Answer (one line) |
|---|------:|---------:|----------:|-----------:|-------------------|
${rows.map(r => r.missing
  ? `| Q${r.q} | — | — | — | — | ⚠️ missing |`
  : `| Q${r.q} | ${r.calls} | ${fmt(r.in)} | ${fmt(r.out)} | ${fmt(r.ms)} | ${trunc(r.one)} |`
).join('\n')}
| **Σ** | **${tot.calls}** | **${fmt(tot.in)}** | **${fmt(tot.out)}** | **${fmt(tot.ms)}** | |
`;

writeFileSync(join(run, 'output.md'), body);
console.log(`wrote ${join(run, 'output.md')}`);
console.log(`questions=${rows.filter(r => !r.missing).length}/31  calls=${tot.calls}  in=${fmt(tot.in)}  out=${fmt(tot.out)}  ms=${fmt(tot.ms)}`);
const missing = rows.filter(r => r.missing).map(r => `Q${r.q}`);
if (missing.length) console.warn(`missing: ${missing.join(', ')}`);
