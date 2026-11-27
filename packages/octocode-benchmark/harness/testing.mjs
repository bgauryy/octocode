// Test support: a scratch folder under .octocode/tmp/ and a small round on disk.
import fs from 'node:fs';
import path from 'node:path';
import { REPO, checkManifest, removeTree } from './lib.mjs';

let serial = 0;

/** A fresh scratch folder for one test, removed by the returned `cleanup`. */
export function scratch(label) {
  serial += 1;
  const dir = path.join(REPO, '.octocode', 'tmp', `bench-harness-test-${process.pid}-${serial}-${label}`);
  removeTree(dir);
  fs.mkdirSync(dir, { recursive: true });
  return { dir, cleanup: () => removeTree(dir) };
}

/** A manifest object for a round stored under `dir`, with one question set of `ids` on disk. */
export function roundIn(dir, { ids = ['Q1', 'Q2', 'Q3'], arms, passes = 1, compare = [], extra = {} } = {}) {
  const questionsDir = path.join(dir, 'questions');
  fs.mkdirSync(path.join(questionsDir, 'answers'), { recursive: true });
  for (const id of ids) fs.writeFileSync(path.join(questionsDir, `${id}.md`), `# ${id} — title of ${id}\n\n## Question\n\nWhat is ${id}?\n\n## Notes\n\nhidden from runners\n`);
  const raw = {
    round: path.basename(dir),
    campaign: path.join(dir, 'campaign'),
    passes,
    jobs: 2,
    neutralCwd: path.join(dir, 'neutral'),
    clones: path.join(dir, 'clones'),
    models: { runner: 'runner-model', judge: 'judge-model' },
    arms: arms ?? {
      tool: { kind: 'mcp', instruction: 'Use the tools.', server: { command: 'node', args: ['server.js'], env: { OCTOCODE_HOME: '{home}' } } },
      gh: { kind: 'shell', instruction: 'Use gh.', filters: ['jq'] },
    },
    compare,
    ...extra,
  };
  raw.sets = { s: { questions: questionsDir, ids, arms: Object.fromEntries(Object.keys(raw.arms).map(arm => [arm, 'all'])) } };
  return checkManifest(raw);
}

/** Writes one stored run: a stream-json transcript from `steps`, result.json and answer.md. */
export function storeRun(dir, { steps, cost = 0, answer = 'the answer', rc = 0, home } = {}) {
  fs.mkdirSync(dir, { recursive: true });
  const lines = [JSON.stringify({ type: 'system', subtype: 'init', tools: ['mcp__octocode__localFetch', 'Bash'] })];
  const usage = { input_tokens: 0, cache_creation_input_tokens: 0, cache_read_input_tokens: 0, output_tokens: 0 };
  steps.forEach((step, at) => {
    const id = `msg_${at}`;
    const stepUsage = { input_tokens: step.input ?? 0, cache_creation_input_tokens: step.cacheWrite ?? 0, cache_read_input_tokens: step.cacheRead ?? 0, output_tokens: 1 };
    usage.input_tokens += stepUsage.input_tokens;
    usage.cache_creation_input_tokens += stepUsage.cache_creation_input_tokens;
    usage.cache_read_input_tokens += stepUsage.cache_read_input_tokens;
    usage.output_tokens += step.output ?? 0;
    const calls = step.calls ?? [];
    if (!calls.length) lines.push(JSON.stringify({ type: 'assistant', message: { id, usage: stepUsage, content: [{ type: 'text', text: answer }] } }));
    calls.forEach((call, i) => lines.push(JSON.stringify({ type: 'assistant', message: { id, usage: stepUsage, content: [{ type: 'tool_use', id: `use_${at}_${i}`, name: call.name, input: call.input }] } })));
    calls.forEach((call, i) => lines.push(JSON.stringify({ type: 'user', message: { content: [{ type: 'tool_result', tool_use_id: `use_${at}_${i}`, is_error: Boolean(call.isError), content: [{ type: 'text', text: call.reply }] }] } })));
  });
  fs.writeFileSync(path.join(dir, 'transcript.jsonl'), `${lines.join('\n')}\n`);
  fs.writeFileSync(path.join(dir, 'answer.md'), answer);
  fs.writeFileSync(path.join(dir, 'result.json'), JSON.stringify({ rc, wall_s: 1, home, result: { num_turns: steps.length, total_cost_usd: cost, usage } }));
}
