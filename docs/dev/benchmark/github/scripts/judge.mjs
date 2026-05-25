#!/usr/bin/env node
// judge.mjs — Score two runs by fact match.
// Heuristic: count, per question, how many Expected Facts (verbatim phrases ≥3 words
// or backticked tokens) appear in each agent's Answer section.
// Usage: node judge.mjs <run_a> <run_b>
import { readFileSync, writeFileSync, existsSync, readdirSync } from 'fs';
import { join, basename, dirname } from 'path';
import { fileURLToPath } from 'url';

const [a, b] = process.argv.slice(2);
if (!a || !b) { console.error('Usage: judge.mjs <run_a> <run_b>'); process.exit(1); }
const here = dirname(fileURLToPath(import.meta.url));
const facts = readFileSync(join(here, '..', 'EXPECTED_FACTS.md'), 'utf8');

const factBlocks = {};
for (const m of facts.matchAll(/^### Q(\d+).*?\n([\s\S]*?)(?=^### Q\d+|^## |\Z)/gm)) {
  factBlocks[+m[1]] = m[2];
}

const tokens = (s) => [
  ...s.matchAll(/`([^`\n]+)`/g),                 // backticked tokens
  ...s.matchAll(/\b([A-Z][A-Za-z0-9_]{3,})\b/g), // CamelCase identifiers
].map(m => m[1]).filter(t => t.length >= 4);

const readAnswer = (run, q) => {
  const p = join(run, `q${q}`, 'output.md');
  if (!existsSync(p)) return '';
  const t = readFileSync(p, 'utf8');
  return (t.split(/## Answer\s*\n+/)[1] || '').toLowerCase();
};

const scoreOne = (answer, factText) => {
  if (!answer) return 0;
  const want = [...new Set(tokens(factText).map(t => t.toLowerCase()))];
  if (!want.length) return 0;
  const hit = want.filter(t => answer.includes(t)).length;
  const ratio = hit / want.length;
  if (ratio >= 0.8) return 3;
  if (ratio >= 0.5) return 2;
  if (ratio >= 0.2) return 1;
  return 0;
};

const rows = [];
let sA = 0, sB = 0;
for (const q of Object.keys(factBlocks).map(Number).sort((x, y) => x - y)) {
  const fa = factBlocks[q];
  const ans_a = readAnswer(a, q), ans_b = readAnswer(b, q);
  const qa = scoreOne(ans_a, fa), qb = scoreOne(ans_b, fa);
  sA += qa; sB += qb;
  rows.push({ q, qa, qb });
}

const aName = basename(a), bName = basename(b);
const out = `# Judge — ${aName} vs ${bName}

| Q | ${aName} | ${bName} |
|---|--------:|--------:|
${rows.map(r => `| Q${r.q} | ${r.qa}/3 | ${r.qb}/3 |`).join('\n')}
| **Σ** | **${sA}/${rows.length * 3}** | **${sB}/${rows.length * 3}** |
`;
const dest = join(dirname(a), `judge-${aName}-vs-${bName}.md`);
writeFileSync(dest, out);
console.log(`wrote ${dest}`);
console.log(`${aName}=${sA}/${rows.length * 3}  ${bName}=${sB}/${rows.length * 3}`);
