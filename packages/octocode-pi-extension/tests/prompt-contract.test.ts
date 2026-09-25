import assert from 'node:assert/strict';
import { test } from 'vitest';
import { buildPlanPrompt } from '../src/prompts/plan-prompt.js';
import { PLAN_PROMPT_MAX_GOAL, PLAN_PROMPT_TRUNCATION_MARKER } from '../src/contracts/prompts/index.js';
import { projectPiSystemPromptCapabilities, SYSTEM_PROMPT } from '../src/prompts/system-prompt.js';
import { SUBAGENT_WORKER_CONTRACT } from '../src/contracts/prompts/index.js';

import { OCTOCODE_MCP_CALL_EXAMPLE } from '../src/tools/octocode-tools.js';


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
  assert.match(prompt, /queries.*action.*propose/is, 'plan mode teaches the query envelope');
  assert.doesNotMatch(prompt, /queries.*reasoning.*action.*propose/is, 'batch labels are optional rather than ceremony');
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

test('product policy only advertises MCP and skill gateways that are active', () => {
  const projected = projectPiSystemPromptCapabilities(SYSTEM_PROMPT, { mcpTool: false, skill: false });
  assert.doesNotMatch(projected, /MCPTool|mcp_catalog_index/i);
  assert.doesNotMatch(projected, /Load a matching Octocode skill/);
  assert.match(projected, /Permissions and approval are host-enforced/);
  assert.equal(projectPiSystemPromptCapabilities(SYSTEM_PROMPT, { mcpTool: true, skill: true }), SYSTEM_PROMPT);
});

test('MCP guidance loads exact schemas and keeps dynamic and fallback calls unambiguous', () => {
  const example = JSON.parse(OCTOCODE_MCP_CALL_EXAMPLE) as {
    queries: Array<{ reasoning?: string; arguments?: { queries?: Array<Record<string, unknown>> } }>;
  };
  assert.equal(example.queries[0]?.reasoning, undefined);
  assert.equal(example.queries[0]?.arguments?.queries?.[0]?.['reasoning'], undefined);
  assert.deepEqual(example.queries[0]?.arguments?.queries, [{ path: '/ABS/repo/README.md', fullContent: true }]);
  assert.match(SYSTEM_PROMPT, /Octocode.*default.*server.*omitted/i);
  assert.match(SYSTEM_PROMPT, /describe/i);
  assert.match(SYSTEM_PROMPT, /exact schema is not active, then call the activated tool/i);
  assert.doesNotMatch(SYSTEM_PROMPT, /outer query owns reasoning|target input stays inside arguments\.queries\[\]/i, 'the active target schema owns exact call shape');
});
