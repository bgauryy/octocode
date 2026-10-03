// Recompute tokens in (command) and out (saved stdout+stderr) per task and side,
// counting only the steps results.json grades. Same tokenizer as bench.mjs.
import fs from 'node:fs';
import { encode } from '../../node_modules/gpt-tokenizer/esm/main.js';
const r = JSON.parse(fs.readFileSync('results.json', 'utf8'));
const runs = fs.readFileSync('runs.jsonl', 'utf8').trim().split('\n').map((l) => JSON.parse(l));
const byKey = new Map(runs.map((x) => [`${x.taskId}|${x.side}|${x.step}`, x]));
const out = {};
const ok = (v) => typeof v === 'string' && (/^yes/.test(v) || /^same/.test(v));
for (const t of r.tasks.filter((t) => t.id !== 'rs1c')) {
  for (const side of ['gh', 'oc']) {
    const steps = t[side]?.steps ?? [];
    let inT = 0, outT = 0, ms = 0, calls = 0;
    for (const s of steps) {
      const x = byKey.get(`${t.id}|${side}|${s}`);
      if (!x) continue;
      const text = fs.existsSync(x.out) ? fs.readFileSync(x.out, 'utf8') : '';
      inT += encode(x.cmd).length; outT += encode(text).length; ms += x.ms; calls++;
    }
    const tool = t.tool;
    const agg = ((out[tool] ??= {})[side] ??= { tasks: 0, correct: 0, inT: 0, outT: 0, ms: 0, calls: 0 });
    agg.tasks++; agg.correct += ok(t[side]?.correct) ? 1 : 0; agg.inT += inT; agg.outT += outT; agg.ms += ms; agg.calls += calls;
  }
}
const all = { gh: { tasks: 0, correct: 0, inT: 0, outT: 0, ms: 0, calls: 0 }, oc: { tasks: 0, correct: 0, inT: 0, outT: 0, ms: 0, calls: 0 } };
for (const v of Object.values(out)) for (const side of ['gh', 'oc']) for (const k of Object.keys(all[side])) all[side][k] += v[side][k];
console.log(JSON.stringify({ perTool: out, all }, null, 1));
