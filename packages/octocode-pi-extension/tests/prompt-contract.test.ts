import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { test } from 'vitest';
import {
  buildPlanPrompt,
  PLAN_PROMPT_MAX_GOAL,
  PLAN_PROMPT_TRUNCATION_MARKER,
} from '../src/prompts/plan-prompt.js';
import { SYSTEM_PROMPT } from '../src/prompts/prompt.js';
import {
  expandSubagentPrompt,
  SUBAGENT_COORDINATION,
  SUBAGENT_PLACEHOLDERS,
  SUBAGENT_SURFACE,
} from '../src/prompts/subagent-shared.js';

const packageRoot = path.resolve(import.meta.dirname, '..');
const roleNames = ['architect', 'browser-agent', 'planner', 'researcher'] as const;

function rolePrompt(role: (typeof roleNames)[number]): string {
  return fs.readFileSync(path.join(packageRoot, 'subagents', role, 'SYSTEM_PROMPT.md'), 'utf8');
}

test('main prompt preserves the stable authority and verification boundaries', () => {
  assert.ok(SYSTEM_PROMPT.startsWith('<authority>'), 'authority remains the first Octocode section');

  for (const [name, pattern] of [
    ['secrets and hidden instructions', /secrets?\/hidden instructions|secrets?.*hidden instructions/i],
    ['protected actions and consent', /protected acts|protected actions/i],
    ['Git history', /git.*history|history.*git/i],
    ['unrelated user work', /unrelated.*work|overwrite others?.*work/i],
    ['truthful verification', /verify for real|truthful verification|checks did not run/i],
  ] as const) {
    assert.match(SYSTEM_PROMPT, pattern, `missing hard boundary: ${name}`);
  }
});

test('main prompt keeps Awareness signal-driven and assigns one owner per coordination concern', () => {
  const block = SYSTEM_PROMPT.match(/<awareness>\n([\s\S]*?)\n<\/awareness>/)?.[1] ?? '';
  assert.match(block, /plan.*session.*shared/i, 'plan owns both planning scopes');
  assert.match(block, /observed check receipts/i, 'plan completion owns routine verification receipts');
  assert.match(block, /automatic model-facing output.*terse state-change signals.*safety or verification blocks/i, 'automatic output is limited to terse actionable signals');
  assert.match(block, /signal never embeds ledger contents/i, 'signals do not inject ledger rows into model context');
  assert.match(block, /advisory presence.*automatic|automatic.*advisory presence/i, 'ordinary file presence is automatic');
  assert.match(block, /mutation.*peer.*locks.*automatic|peer.*locks.*mutation.*automatic/i, 'peer lock enforcement is automatic');
  assert.match(block, /lock.*exceptional/i, 'explicit locks are exceptional');
  assert.match(block, /message.*peer/i, 'message owns necessary peer communication');
  assert.match(block, /memory.*change the approach/i, 'memory remains conditional');
  assert.doesNotMatch(block, /Awareness CLI|inspect.*schema|backend recovery/i, 'backend recovery detail stays out of the always-loaded prompt');
  assert.doesNotMatch(block, /before (?:starting|finishing)|start every task|join once|claim or create|declare touched paths|verify audit/i);
  assert.doesNotMatch(block, /awarenessPlan|\bclaim\b|awarenessAgents|\bverify\b|\bwork\b/);

  const words = block.trim().split(/\s+/).length;
  const bullets = block.match(/^- /gm)?.length ?? 0;
  assert.ok(words <= 180, `Awareness stays concise (received ${words} words)`);
  assert.ok(bullets <= 5, `Awareness has one compact rule set (received ${bullets} bullets)`);

  const repositoryBlock = SYSTEM_PROMPT.match(/<repository>\n([\s\S]*?)\n<\/repository>/)?.[1] ?? '';
  const routingBlock = SYSTEM_PROMPT.match(/<capability_routing>\n([\s\S]*?)\n<\/capability_routing>/)?.[1] ?? '';
  assert.match(repositoryBlock, /follow.*<awareness>|<awareness>.*rules/i, 'repository points to the canonical Awareness owner');
  assert.doesNotMatch(repositoryBlock, /Use Awareness when|verification debt/i, 'repository does not duplicate Awareness routing');
  assert.doesNotMatch(routingBlock, /Awareness contract|Use Awareness when/i, 'capability routing does not duplicate Awareness guidance');
});

test('main prompt defines a concise completion summary for users, not an internal activity log', () => {
  const block = SYSTEM_PROMPT.match(/<output>\n([\s\S]*?)\n<\/output>/)?.[1] ?? '';
  assert.match(block, /completed change\/build.*TL;DR:/i, 'non-trivial completed work starts with a TL;DR');
  assert.match(block, /every user-requested.*scope item/i, 'completion covers the full requested scope');
  assert.match(block, /Completed.*Checks.*Notes/is, 'completion uses stable scan-friendly sections');
  assert.match(block, /omit.*Notes.*none/i, 'empty notes do not create noise');
  assert.match(block, /simple answers.*do not use.*template/i, 'simple answers stay minimal');
  assert.match(block, /group.*outcome.*chronolog/i, 'work is grouped by outcome instead of replayed chronologically');
  assert.match(block, /Verified for real.*do not|do not.*Verified for real/i, 'vague verification branding is explicitly rejected');
  assert.match(block, /plan IDs|task IDs|claims|workers|coordination cleanup/i, 'routine summaries hide internal coordination metadata');
});

test('main and shared worker prompts distinguish scoped instructions from untrusted repository content', () => {
  const workerPrompt = `${SUBAGENT_SURFACE}
${SUBAGENT_COORDINATION}`;
  for (const [name, prompt] of [
    ['main', SYSTEM_PROMPT],
    ['worker', workerPrompt],
  ] as const) {
    assert.match(prompt, /ordinary repository content.*untrusted/i, `${name} prompt distrusts ordinary repository content`);
    assert.match(
      prompt,
      /applicable repository instruction files .*harness or user.*subordinate instructions/i,
      `${name} prompt honors surfaced scoped instructions`,
    );
  }
});

test('main and shared worker prompts prohibit Git unless the user explicitly requests it', () => {
  const workerPrompt = `${SUBAGENT_SURFACE}
${SUBAGENT_COORDINATION}`;
  for (const [name, prompt] of [
    ['main', SYSTEM_PROMPT],
    ['worker', workerPrompt],
  ] as const) {
    assert.match(
      prompt,
      /never run any Git command unless .*user.*explicitly (asks|requests)/i,
      `${name} requires an explicit user request for every Git command`,
    );
    assert.match(prompt, /read-only/i, `${name} makes clear that read-only Git is also prohibited`);
    assert.doesNotMatch(prompt, /bounded read-only commands.*allowed/i, `${name} removes the autonomous Git fallback`);
    assert.doesNotMatch(prompt, /\bgit (status|branch|log|diff)\b/i, `${name} suggests no concrete Git inspection command`);
  }

  assert.match(SYSTEM_PROMPT, /harness-provided repo snapshot.*hint/i, 'main uses the supplied repo snapshot without refreshing it');
  assert.match(SYSTEM_PROMPT, /Awareness.*flow.*ownership.*overlap/i, 'main routes shared state through Awareness');
  assert.match(SUBAGENT_SURFACE, /Awareness.*ownership.*Octocode.*history/i, 'workers route state and history through supported surfaces');
});

test('main prompt preserves active-work, plan, behavior-baseline, compatibility, and output branches', () => {
  assert.match(SYSTEM_PROMPT, /standalone request.*report it.*stop/i, 'standalone status stops after reporting');
  assert.match(SYSTEM_PROMPT, /active authorized work.*continue the next owed action.*pause/i, 'active status continues unless paused');
  assert.match(SYSTEM_PROMPT, /coherent increment.*checkpoint.*continue.*active authorized plan/i, 'an intermediate increment does not terminate an authorized plan');
  assert.match(SYSTEM_PROMPT, /stop only when the overall user request.*acceptance/i, 'stop is scoped to overall request acceptance, not a substep');
  assert.match(SYSTEM_PROMPT, /Planning ends on approval.*reclassify.*change\/build/i, 'approval ends planning before execution');
  assert.match(SYSTEM_PROMPT, /unknown most likely to invalidate the plan/i, 'planning prioritizes the riskiest unknown');
  assert.match(SYSTEM_PROMPT, /observable behavior change.*failing check or behavioral baseline/i, 'behavior changes establish evidence first');
  assert.match(SYSTEM_PROMPT, /existing accepted contract or explicit user requirement/i, 'accepted contracts may require compatibility');
  assert.match(SYSTEM_PROMPT, /top-down.*entrypoints.*contracts.*bottom-up.*implementations.*data flow.*control flow.*Reconcile both views/i, 'non-trivial code understanding reconciles top-down and bottom-up evidence');
  assert.match(SYSTEM_PROMPT, /Respond in the user's language/i, 'responses match the user language');
  assert.match(SYSTEM_PROMPT, /request is complete.*stop cleanly.*Do not append generic offers.*next tasks/i, 'completed responses do not invent follow-up work');
  assert.match(SYSTEM_PROMPT, /long-running work.*state, a blocker, or the next action changes/i, 'long work reports meaningful changes');
  assert.match(SYSTEM_PROMPT, /intermediate increment.*final-style recap.*continue/i, 'intermediate work does not emit a verbose completion answer');
  assert.match(SYSTEM_PROMPT, /worker.*\[DONE\].*delegated.*not.*parent.*request.*continue/i, 'worker completion does not terminate parent plan execution');
  assert.match(SYSTEM_PROMPT, /do not narrate every tool call or use a fixed timer/i, 'progress remains quiet and event-driven');
});

test('main prompt makes compacted checkpoints subordinate to current plan and docs', () => {
  const recoveryGuidance = SYSTEM_PROMPT.match(
    /After compaction or session rehydration,[\s\S]*?(?=\n\nImprovement needs a sensor:)/i,
  )?.[0] ?? '';

  assert.match(recoveryGuidance, /checkpoint text.*recovery\s+hint/i);
  assert.match(recoveryGuidance, /current active plan/i);
  assert.match(recoveryGuidance, /referenced docs/i);
  assert.match(recoveryGuidance, /current sources/i);
  assert.match(recoveryGuidance, /stale saved text/i);
  assert.match(recoveryGuidance, /complete.*blocked on approval.*waiting for the user.*stop/is);
});

test('main prompt keeps research, tests, comments, and reflection evidence-efficient', () => {
  const localBlock = SYSTEM_PROMPT.match(/<local_tools>\n([\s\S]*?)\n<\/local_tools>/)?.[1] ?? '';
  const routingBlock = SYSTEM_PROMPT.match(/<capability_routing>\n([\s\S]*?)\n<\/capability_routing>/)?.[1] ?? '';
  const qualityBlock = SYSTEM_PROMPT.match(/<code_quality>\n([\s\S]*?)\n<\/code_quality>/)?.[1] ?? '';
  const judgmentBlock = SYSTEM_PROMPT.match(/<judgment>\n([\s\S]*?)\n<\/judgment>/)?.[1] ?? '';

  assert.match(localBlock, /Markdown.*minify:["']symbols["'].*skeleton/i, 'Markdown starts from the supported symbols skeleton');
  assert.match(localBlock, /skeleton.*choose.*exact.*region|choose.*exact.*region.*skeleton/i, 'the skeleton drives focused exact reads');
  assert.doesNotMatch(
    localBlock,
    /captureText|metavarRanges|orderHint|charOffset|next\.\*/i,
    'live tool-schema field metadata stays out of the stable prompt',
  );
  assert.match(routingBlock, /bundled MCP.*npx octocode tools|npx octocode tools.*bundled MCP/i, 'both supported Octocode tool surfaces are named');

  assert.match(qualityBlock, /TDD|test-driven/i, 'TDD remains explicit');
  assert.match(qualityBlock, /smallest.*decision-changing|decision-changing.*smallest/i, 'tests stay focused and efficient');
  assert.match(qualityBlock, /observable.*contract.*implementation|implementation.*observable.*contract/i, 'tests prefer behavior over implementation calls');
  assert.match(qualityBlock, /mock.*external.*nondeterministic.*narrow|narrow.*mock.*external.*nondeterministic/i, 'external and nondeterministic mocks stay at narrow seams');
  assert.match(qualityBlock, /real.*internal.*collaborator|internal.*collaborator.*real/i, 'cheap deterministic internals stay real');
  assert.match(qualityBlock, /table.*cases.*shar|combin.*cases.*shar/i, 'cases with the same setup and outcome are combined');
  assert.match(qualityBlock, /redundant.*equivalent coverage|equivalent coverage.*redundant/i, 'redundant tests require coverage proof before removal');
  assert.match(qualityBlock, /skips?.*live.*platform.*condition.*reason|live.*platform.*skips?.*condition.*reason/i, 'skips are explicit capability gates');
  assert.match(qualityBlock, /comments.*JSDoc.*explanations/i, 'comments and JSDoc are maintained explanations');
  assert.match(qualityBlock, /why.*non-obvious|non-obvious.*why/i, 'comments explain why instead of narrating syntax');

  assert.match(judgmentBlock, /self-critique|challenge.*hypothesis/i, 'operations include proportional self-critique');
  assert.match(
    judgmentBlock,
    /material steps.*decisions.*observed evidence.*assumptions or inferences/i,
    'material actions and decisions stay evidence-derived while uncertainty remains explicit',
  );
  assert.doesNotMatch(judgmentBlock, /\bprobe\b/i, 'decision guidance uses plain action language instead of probe jargon');
  assert.match(judgmentBlock, /\.octocode\/REFLECT\.md/i, 'the requested reflection path is handled explicitly');
  assert.match(judgmentBlock, /workspace.*\.octocode\/REFLECT\.md|\.octocode\/REFLECT\.md.*workspace/i, 'reflection is workspace-local');
  assert.match(judgmentBlock, /~\/\.octocode.*global|global.*~\/\.octocode/i, 'global Octocode home stays distinct');
  assert.match(judgmentBlock, /do not invent|never.*hand-edit.*\.octocode/i, 'generated Awareness state is not fabricated');
  assert.match(judgmentBlock, /memory.*verified.*reusable|verified.*reusable.*memory/i, 'durable memory stays evidence-gated');
});

test('main and worker prompts make parallel delegation dependency-aware and overlap-safe', () => {
  assert.match(SYSTEM_PROMPT, /before delegating.*dependency graph/i, 'parent maps ordering before spawning');
  assert.match(
    SYSTEM_PROMPT,
    /two or more.*independent.*disjoint write ownership.*parallel/i,
    'independent non-overlapping lanes should run in parallel',
  );
  assert.match(SYSTEM_PROMPT, /dependent.*shared-file.*serial/i, 'coupled work stays serial');
  assert.match(
    SYSTEM_PROMPT,
    /parent.*must not edit.*delegated paths.*released|delegated paths.*parent.*must not edit.*released/i,
    'the parent stays out of active worker ownership',
  );
  assert.match(SYSTEM_PROMPT, /overlap.*stop.*reassign/i, 'new overlap is resolved before work resumes');

  assert.match(SUBAGENT_COORDINATION, /edit only.*assigned.*Ownership/i, 'workers stay inside explicit ownership');
  assert.match(SUBAGENT_COORDINATION, /overlap.*stop before writing.*parent/i, 'workers fail closed before overlapping edits');
  assert.match(SUBAGENT_COORDINATION, /research-only.*must not mutate/i, 'research workers remain read-only');
  assert.doesNotMatch(SUBAGENT_COORDINATION, /coordinate ordinary overlap/i, 'workers never race and merge later');
});

test('main prompt scopes mathematical modeling to useful diagnosis evidence', () => {
  assert.match(
    SYSTEM_PROMPT,
    /For diagnosis work.*mathematical modeling only when measurable quantities or explicit relationships can change the diagnosis, fix, or verification/i,
    'quantitative modeling must change a diagnosis outcome',
  );
  assert.match(
    SYSTEM_PROMPT,
    /define variables, units, constraints, assumptions, and uncertainty.*validate only calculations supported by evidence/i,
    'quantitative diagnosis names inputs and validates supported calculations',
  );
  assert.match(
    SYSTEM_PROMPT,
    /otherwise use direct causal reasoning without forced mathematical framing/i,
    'qualitative diagnosis keeps a direct causal path',
  );
  assert.doesNotMatch(
    SYSTEM_PROMPT,
    /Return the root cause, supporting evidence, smallest viable fix/i,
    'diagnosis guidance does not override global output modes',
  );
});

test('main prompt top-level XML sections are balanced, uniquely owned, and remain compact', () => {
  const opens = [...SYSTEM_PROMPT.matchAll(/^<([a-z_]+)>$/gm)].map((match) => match[1]);
  const closes = [...SYSTEM_PROMPT.matchAll(/^<\/([a-z_]+)>$/gm)].map((match) => match[1]);
  assert.deepEqual(closes, opens, 'top-level sections close in the same order they open');
  assert.equal(new Set(opens).size, opens.length, 'each top-level section has one owner');
  const sections = [...SYSTEM_PROMPT.matchAll(/^<([a-z_]+)>\n([\s\S]*?)\n<\/\1>$/gm)];
  assert.equal(sections.length, opens.length, 'every owned section is structurally parseable');
  for (const [, name, body] of sections) assert.ok(body!.trim(), `<${name}> has behavior-defining content`);

  for (const removedSection of [
    'octocode_cli',
    'skills',
    'agents',
    'tools',
    'ui_ux',
    'browser_agent',
    'search_and_research',
    'code',
    'testing',
    'ultimate_reminders',
  ]) {
    assert.ok(!opens.includes(removedSection), `static <${removedSection}> section stays externalized`);
  }
});

test('plan mode keeps its no-mutation and explicit approval gate', () => {
  const prompt = buildPlanPrompt('change the public API');
  assert.match(prompt, /PLAN MODE/i);
  assert.match(prompt, /do not (?:edit|change)|must not edit|no mutation/i);
  assert.match(prompt, /accept(?:ance)?.*does not.*authoriz.*implementation/i);
  assert.match(prompt, /separate.*start/i);
  assert.match(prompt, /rejected|rejection/i);
  assert.match(prompt, /plan\(propose\)|action:\s*["']propose["']/i);
  assert.match(prompt, /terminal Summary first.*open the full browser review/i);
  assert.match(prompt, /Do not change code before.*Start/i);
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

test('typed-worker coordination treats assigned ownership as exclusive', () => {
  assert.match(SUBAGENT_COORDINATION, /never edit through an exclusive lock or another owner's active path/i);
  assert.match(SUBAGENT_COORDINATION, /overlaps active parent or peer ownership.*stop before writing/i);
  assert.match(SUBAGENT_COORDINATION, /wait for an explicit release or reassignment/i);
  assert.doesNotMatch(SUBAGENT_COORDINATION, /Coordinate ordinary overlap/i);
});

test('all typed role prompts expand the same shared protocol and preserve parser terminal states', () => {
  const coordinationBlocks: string[] = [];
  for (const role of roleNames) {
    const source = rolePrompt(role);
    const expanded = expandSubagentPrompt(source);

    for (const placeholder of SUBAGENT_PLACEHOLDERS) {
      assert.doesNotMatch(expanded, new RegExp(placeholder.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')));
    }
    assert.ok(expanded.includes(SUBAGENT_COORDINATION), `${role} receives the canonical coordination block`);
    assert.match(expanded, /\[DONE\]/, `${role} preserves DONE`);
    assert.match(expanded, /\[BLOCKED\]/, `${role} preserves BLOCKED`);
    assert.match(expanded, /\[FAILED\]/, `${role} preserves FAILED`);
    assert.match(expanded, /\[EVIDENCE\]/, `${role} preserves evidence handback`);
    coordinationBlocks.push(SUBAGENT_COORDINATION);
  }
  assert.equal(new Set(coordinationBlocks).size, 1, 'one shared worker protocol owns coordination');
});
