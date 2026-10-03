#!/usr/bin/env node
// After all questions: build one REFLECT.md per worker from that worker's own per-question
// reflections (written by run.mjs). The worker model synthesizes them in a fresh session with
// no tools. Judge verdicts and reference answers are never shown to it.
//
//   node reflect.mjs --run-id <id> [--model sonnet]
import fs from 'node:fs';
import path from 'node:path';
import { RESULTS_DIR, REPO_ROOT, commonFlags, freshCwd, loadWorkers, parseArgs, parseStream, runClaude, tokenAccounting, readJson } from './lib.mjs';
import { solverBoundary, evaluatorCredentials } from './isolation.mjs';

const args = parseArgs(process.argv.slice(2), { model: 'sonnet' });
if (!/^[\w-]+$/.test(String(args['run-id'] ?? ''))) throw new Error('valid --run-id is required');
if (!String(args.model).startsWith('claude-')) throw new Error('concrete --model is required');
const runDir = path.join(RESULTS_DIR, String(args['run-id']));
const runsDir = path.join(runDir, 'runs');
const manifest = readJson(path.join(runDir, 'manifest.json'));
const credentials = evaluatorCredentials();

for (const worker of loadWorkers(manifest.workers.join(','))) {
  const notes = [];
  for (const qid of manifest.questionIds) {
    const p = path.join(runsDir, qid, worker.id, 'run.json');
    if (!fs.existsSync(p)) throw new Error(`missing reflection input ${qid}/${worker.id}`);
    const record = readJson(p);
    if (!record.valid || !record.reflection?.valid) throw new Error(`invalid reflection input ${qid}/${worker.id}`);
    const text = record.reflection.text?.trim();
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
  let res, boundary;
  try {
    boundary = await solverBoundary({ cwd, corpus: [], repoRoot: REPO_ROOT, ...credentials });
    const doc = path.join(cwd, 'WORKER.md'); fs.copyFileSync(worker.docPath, doc);
    res = await runClaude({
      args: ['-p', prompt, ...commonFlags({ model: String(args.model), maxTurns: 2 }), '--append-system-prompt-file', doc, '--strict-mcp-config', '--tools', ''],
      cwd, timeoutMs: 10 * 60_000, streamPath: path.join(outDir, 'synthesis.stream.jsonl'), env: boundary.env, sandboxProfile: boundary.sandboxProfile,
    });
  } finally { await boundary?.close(); fs.rmSync(cwd, { recursive: true, force: true }); }
  const m = parseStream(res.stream);
  if (res.exitCode !== 0 || res.signal || res.timedOut || m.isError || !m.costVerified || m.resultSubtype !== 'success' || m.toolCalls.length || !tokenAccounting(m).verified) throw new Error(`invalid reflection synthesis ${worker.id}`);
  fs.writeFileSync(path.join(outDir, 'REFLECT.md'), `# ${worker.id}: reflection after ${notes.length} questions\n\n${m.answer}\n\n---\n\n# Per-question reflections\n\n${raw}\n`);
  fs.writeFileSync(path.join(outDir, 'synthesis.json'), JSON.stringify({ tokens: tokenAccounting(m), cost_usd: m.total_cost_usd, costVerified: m.costVerified }, null, 1));
  console.log(`${worker.id}: REFLECT.md from ${notes.length} reflections ($${m.total_cost_usd.toFixed(3)})`);
}
