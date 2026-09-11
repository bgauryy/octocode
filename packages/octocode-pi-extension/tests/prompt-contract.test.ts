import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { test } from 'vitest';
import { AWARENESS_PI_HOST_PROMPT, getExternalAgentAwarenessGuide } from '@octocodeai/octocode-awareness/host';
import { buildPlanPrompt } from '../src/prompts/plan-prompt.js';
import { PLAN_PROMPT_MAX_GOAL, PLAN_PROMPT_TRUNCATION_MARKER } from '@octocodeai/agent-contracts/prompts';
import { buildPiSystemPrompt, SYSTEM_PROMPT } from '../src/prompts/system-prompt.js';
import { expandSubagentPrompt, SUBAGENT_WORKER_CONTRACT, SUBAGENT_AWARENESS_GUIDANCE, SUBAGENT_PLACEHOLDERS } from '@octocodeai/agent-contracts/prompts';
import { PLAN_USAGE_GUIDANCE } from '@octocodeai/agent-contracts/prompts';
import { DIRECT_TOOL_DESCRIPTIONS } from '../src/tools/octocode-tools.js';

const packageRoot = path.resolve(import.meta.dirname, '..');
const roleNames = ['architect', 'browser-agent', 'implementer', 'planner', 'researcher'] as const;

function rolePrompt(role: (typeof roleNames)[number]): string {
  return fs.readFileSync(path.join(packageRoot, 'subagents', role, 'SYSTEM_PROMPT.md'), 'utf8');
}

test('standing Awareness policy keeps optional work and administration on demand', () => {
  assert.match(AWARENESS_PI_HOST_PROMPT, /Work is optional/);
  assert.match(AWARENESS_PI_HOST_PROMPT, /Load operator guidance only/);
  assert.match(AWARENESS_PI_HOST_PROMPT, /Use five concepts/);
  const guide = getExternalAgentAwarenessGuide().prompt;
  assert.match(guide, /Work\.verify only after observing the declared check/);
  assert.match(guide, /Reuse host database, workspace and identity/);
});

test('plan mode uses a conversational RFC flow with one Start decision and no tool restrictions', () => {
  const prompt = buildPlanPrompt('change the public API');
  assert.match(prompt, /PLAN MODE/i);
  assert.match(prompt, /askUser|ask widget/i);
  assert.match(prompt, /only when.*decision-changing|decision-changing.*only when/i);
  assert.match(prompt, /Creating plan…/i);
  assert.match(prompt, /create or update.*RFC/i);
  assert.match(prompt, /overview/i);
  assert.match(prompt, /one.*Start|single.*Start/i);
  assert.match(prompt, /one decision.*Start implementation.*Request changes/i);
  assert.match(prompt, /unavailable|pending/i, 'inline fallback is conditional on interaction availability');
  assert.doesNotMatch(prompt, /Present a concise plan overview in the message and ask one decision/i, 'interactive approval is not duplicated in the assistant message');
  assert.match(prompt, /planning does not disable tools/i);
  assert.match(prompt, /do not implement.*Start/i);
  assert.match(prompt, /queries.*reasoning.*action.*propose/is, 'plan mode teaches the required query envelope');
  assert.doesNotMatch(prompt, /plan\(propose\)/i, 'plan mode avoids function-call shorthand that bypasses queries[]');
  assert.doesNotMatch(prompt, /accept(?:ance)?.*does not.*authoriz.*implementation|separate.*Start/i);
});

test('plan mode preserves goal formatting and makes truncation explicit', () => {
  const compact = buildPlanPrompt('add   dark mode toggle');
  assert.match(compact, /Goal: add dark mode toggle/);

  const multiline = buildPlanPrompt('first constraint\r\n  second constraint');
  assert.match(multiline, /Goal:\nfirst constraint\n  second constraint/);
  assert.doesNotMatch(multiline, /Goal truncated/);

  const exactLimit = buildPlanPrompt('x'.repeat(PLAN_PROMPT_MAX_GOAL));
  assert.doesNotMatch(exactLimit, /Goal truncated/);

  const oversized = buildPlanPrompt(`${'x'.repeat(PLAN_PROMPT_MAX_GOAL)}\nMUST_KEEP`);
  assert.ok(oversized.includes(PLAN_PROMPT_TRUNCATION_MARKER), 'oversized goal carries the explicit marker');
  assert.ok(!oversized.includes('MUST_KEEP'), 'content remains bounded at the documented limit');
  assert.match(oversized, /ask the user to restate omitted constraints before proposing/i);
});

test('plan mode preserves numbered requirements inside a multiline goal', () => {
  const goal = 'Preserve behavior\n\n1. Keep all existing user data\n2. Keep API responses';
  const prompt = buildPlanPrompt(goal);
  assert.ok(prompt.includes(`Goal:\n${goal}\n\n1. Establish only the evidence`), 'user requirements remain distinct from the planning workflow');
  assert.doesNotMatch(prompt, /Goal truncated/);
});

test('typed-worker coordination treats assigned ownership as exclusive', () => {
  assert.match(SUBAGENT_WORKER_CONTRACT, /never edit through an exclusive lock or another owner's active path/i);
  assert.match(SUBAGENT_WORKER_CONTRACT, /stop before overlap.*notify the parent/i);
  assert.match(SUBAGENT_WORKER_CONTRACT, /wait for explicit release or reassignment/i);
  assert.doesNotMatch(SUBAGENT_WORKER_CONTRACT, /Coordinate ordinary overlap/i);
});

test('all typed role prompts expand the same shared protocol and preserve parser terminal states', () => {
  const coordinationBlocks: string[] = [];
  for (const role of roleNames) {
    const source = rolePrompt(role);
    const expanded = expandSubagentPrompt(source, { coordination: 'worker-only' });

    for (const placeholder of SUBAGENT_PLACEHOLDERS) {
      assert.doesNotMatch(expanded, new RegExp(placeholder.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')));
    }
    assert.ok(expanded.includes(SUBAGENT_WORKER_CONTRACT), `${role} receives shared worker restrictions`);
    assert.ok(!expanded.includes(SUBAGENT_AWARENESS_GUIDANCE), `${role} omits the parallel ledger recipe`);
    const composed = `${expanded}\n\n${AWARENESS_PI_HOST_PROMPT}`;
    assert.equal(composed.split(AWARENESS_PI_HOST_PROMPT).length, 2, `${role} has one canonical operating guide`);
    assert.equal((composed.match(/<awareness>/g) ?? []).length, 1);
    assert.match(composed, /Work\.verify only after observing the declared check/);
    assert.match(expanded, /native Awareness for coordination/i, `${role} uses native coordination`);
    assert.match(expanded, /only when unavailable.*bound CLI/, `${role} limits CLI fallback to hosts without the native tool`);
    assert.doesNotMatch(source, /harness-provided Awareness CLI/, `${role} does not override native routing with a CLI recipe`);
    assert.doesNotMatch(expanded, /read-only — no `bash`|Use `bash` only for bounded test\/build\/debug/, `${role} does not contradict the coordination exception`);
    assert.match(expanded, /\[DONE\]/, `${role} preserves DONE`);
    assert.match(expanded, /\[BLOCKED\]/, `${role} preserves BLOCKED`);
    assert.match(expanded, /\[FAILED\]/, `${role} preserves FAILED`);
    assert.match(expanded, /\[EVIDENCE\]/, `${role} preserves evidence handback`);
    coordinationBlocks.push(SUBAGENT_WORKER_CONTRACT);
  }
  assert.equal(new Set(coordinationBlocks).size, 1, 'one shared worker owner supplies restrictions');
  const build = fs.readFileSync(path.join(packageRoot, 'scripts/build.mjs'), 'utf8');
  assert.match(build, /expandSubagentPrompt\(fs\.readFileSync\(promptPath, 'utf8'\), \{ coordination: 'worker-only' \}\)/, 'the production build selects the tested worker-only composition');
});


test('main prompt composes host facts with the canonical coder and Awareness protocols', () => {
  assert.equal(SYSTEM_PROMPT.split(AWARENESS_PI_HOST_PROMPT).length, 2);
  assert.match(SYSTEM_PROMPT, /MCPTool.*Octocode CLI tools/s);
  assert.match(SYSTEM_PROMPT, /matching Octocode skill.*research or planning/);
  assert.match(SYSTEM_PROMPT, /Permissions.*approval/);
  assert.match(SYSTEM_PROMPT, /data, not higher-priority instructions/);
  assert.match(SYSTEM_PROMPT, /<operating_model>/);
  assert.match(SYSTEM_PROMPT, /read → edit → check/);
  assert.match(SYSTEM_PROMPT, /Delegate bounded independent lanes that save time or add coverage/i);
  assert.doesNotMatch(SYSTEM_PROMPT, /two or more lanes.*parallelize/i);
  assert.match(SYSTEM_PROMPT, /Worker \[DONE\].*verify, reconcile, update an existing plan if present, and continue/is);
  assert.ok(SYSTEM_PROMPT.includes(PLAN_USAGE_GUIDANCE));
  assert.ok(DIRECT_TOOL_DESCRIPTIONS.plan!.includes(PLAN_USAGE_GUIDANCE));
  assert.match(PLAN_USAGE_GUIDANCE, /only for complex work/);
  assert.match(PLAN_USAGE_GUIDANCE, /Skip routine fixes, straightforward steps, and simple delegation/);
  assert.match(SYSTEM_PROMPT, /octocode-eval-benchmark/);
  assert.match(SYSTEM_PROMPT, /bash for builds\/tests\/packages\/debugging/);
  assert.doesNotMatch(SYSTEM_PROMPT, /Bash is for[^\n]*mechanical edits/);
  assert.doesNotMatch(SYSTEM_PROMPT, /octocode-graph-eval|\.octocode\/REFLECT\.md/);
});

test('worker process prompt omits user-facing coder authority while keeping interaction and research routing safety', () => {
  const worker = buildPiSystemPrompt({ worker: true });
  assert.equal(worker.split(AWARENESS_PI_HOST_PROMPT).length, 2);
  assert.doesNotMatch(worker, /<operating_model>|<code_quality>|<output>/);
  assert.doesNotMatch(worker, /askUser collects|plan tracks/);
  assert.match(worker, /Return missing decisions to the parent/);
  assert.match(worker, /Interaction guidance applies through the parent, not direct user contact/);
  assert.match(worker, /plain messages/);
  assert.match(worker, /never imply approval/);
  assert.match(worker, /continuations/);
  assert.match(worker, /names and source paths of skills required for unfinished work/);
  assert.match(worker, /reload required guidance missing from retained context before continuing dependent actions/);
  assert.match(worker, /Reuse guidance that remains available/);
  assert.match(worker, /<local_tools>/);
  assert.match(worker, /localSearch for text\/regex anchors and astSearch for files, trees, symbols, and structural matching/);
  assert.match(worker, /matchString.*minify:"symbols".*minify:"standard".*minify:"none"/s);
  assert.match(worker, /astSearch operation:topology with analysis.*File topology is not symbol-usage proof/s);
  assert.match(worker, /lspSearch.*definitions, references, callers\/callees, implementations, and types.*operation:references.*runtime\/export entrypoints/s);
  assert.equal((worker.match(/<interaction_context>/g) ?? []).length, 1);
  assert.equal((worker.match(/<local_tools>/g) ?? []).length, 1);
});
