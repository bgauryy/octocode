#!/usr/bin/env node
// After all questions: build one REFLECT.md per worker from that worker's own per-question
// reflections (written by run.mjs). The worker model synthesizes them in a fresh session with
// no tools. Judge verdicts and reference answers are never shown to it.
//
//   node reflect.mjs --run-id <id> [--model sonnet]
import fs from 'node:fs';
import path from 'node:path';
import { RESULTS_DIR, commonFlags, freshCwd, loadWorkers, parseArgs, parseStream, runClaude, tokenAccounting } from './lib.mjs';

const args = parseArgs(process.argv.slice(2), { model: 'sonnet' });
if (!args['run-id']) throw new Error('--run-id is required');
const runDir = path.join(RESULTS_DIR, String(args['run-id']));
const runsDir = path.join(runDir, 'runs');

for (const worker of loadWorkers('all')) {
  const notes = [];
  for (const qid of fs.existsSync(runsDir) ? fs.readdirSync(runsDir).sort() : []) {
    const p = path.join(runsDir, qid, worker.id, 'run.json');
    if (!fs.existsSync(p)) continue;
    const text = JSON.parse(fs.readFileSync(p, 'utf8')).reflection?.text?.trim();
    if (text) notes.push({ qid, text });
  }
  if (!notes.length) { console.log(`${worker.id}: no reflections`); continue; }
  const outDir = path.join(runDir, 'reflections', worker.id);
  fs.mkdirSync(outDir, { recursive: true });
  const raw = notes.map((n) => `## ${n.qid}\n\n${n.text}`).join('\n\n');
  const prompt = `You answered ${notes.length} code-research questions, one per session, using the tools described in your instructions. After each one you wrote the reflection below. Do not call any tools.

Write a REFLECT.md for the people improving your tools and instructions, with these sections:
- **What helped:** tools, queries and habits that worked, with the question ids where they did.
- **What did not help:** wasted calls, noisy or unexpected output, errors, missing capabilities, with question ids.
- **Patterns:** anything that recurred across questions.
- **Suggested changes:** concrete changes to your tools or instructions, most valuable first.
Only use what the reflections say; do not invent results.

<reflections>
${raw}
</reflections>`;
  const cwd = freshCwd(`reflect-${worker.id}`);
  const res = await runClaude({
    args: ['-p', prompt, ...commonFlags({ model: String(args.model), maxTurns: 1 }), '--append-system-prompt-file', worker.docPath, '--strict-mcp-config', '--tools', ''],
    cwd, timeoutMs: 10 * 60_000, streamPath: path.join(outDir, 'synthesis.stream.jsonl'),
  });
  fs.rmSync(cwd, { recursive: true, force: true });
  const m = parseStream(res.stream);
  fs.writeFileSync(path.join(outDir, 'REFLECT.md'), `# ${worker.id}: reflection after ${notes.length} questions\n\n${m.answer}\n\n---\n\n# Per-question reflections\n\n${raw}\n`);
  fs.writeFileSync(path.join(outDir, 'synthesis.json'), JSON.stringify({ tokens: tokenAccounting(m.perRequest), cost_usd: m.total_cost_usd }, null, 1));
  console.log(`${worker.id}: REFLECT.md from ${notes.length} reflections ($${m.total_cost_usd.toFixed(3)})`);
}
