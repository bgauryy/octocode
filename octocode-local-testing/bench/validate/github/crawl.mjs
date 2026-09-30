#!/usr/bin/env node
// crawl.mjs <task> <tool> <keys comma-separated> : starting from out/<task>.oc.1.txt, replay verbatim
// (a) top-level responsePagination.next.query (whole envelope) and
// (b) results[0].data.next[<key>].query for each key in the list
// until no new continuation remains. Each call is recorded through run.mjs.
import { readFileSync, writeFileSync, existsSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
const here = dirname(fileURLToPath(import.meta.url));
const [task, tool, keysArg] = process.argv.slice(2);
const keys = (keysArg || 'nextPage').split(',');
const seen = new Set();
const queue = [1];
let step = 1;
while (existsSync(join(here, `out/${task}.oc.${step + 1}.txt`))) step++; // resume
const parse = (s) => { const txt = readFileSync(join(here, `out/${task}.oc.${s}.txt`), 'utf8').split('\n--- STDERR ---')[0]; try { return JSON.parse(txt); } catch { return null; } };
const pending = [];
for (let s = 1; s <= step; s++) pending.push(s);
while (pending.length) {
  const s = pending.shift();
  const j = parse(s); if (!j) continue;
  const envs = [];
  const rp = j.responsePagination?.next?.query; if (rp) envs.push(rp);
  const d = j.results?.[0]?.data ?? {};
  for (const k of keys) {
    const q = d.next?.[k]?.query ?? d.pullRequests?.[0]?.next?.[k]?.query;
    if (q) envs.push({ queries: [q] });
  }
  for (const env of envs) {
    const key = JSON.stringify(env);
    if (seen.has(key)) continue; seen.add(key);
    step++;
    const qf = join(here, `out/${task}.oc.${step}.query.json`);
    writeFileSync(qf, key);
    const out = execFileSync('node', [join(here, 'run.mjs'), task, 'oc', String(step), `octocode ${tool} "$(cat ${qf})"`], { encoding: 'utf8', env: { ...process.env, SHOW: '0' } });
    process.stdout.write(`step ${step}: ${out.split('\n')[0]}\n`);
    pending.push(step);
  }
}
