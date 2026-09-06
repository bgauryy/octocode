import {
  PLAN_PROMPT_MAX_GOAL,
  PLAN_PROMPT_TRUNCATION_MARKER,
} from '@octocodeai/agent-contracts/prompts';

export function buildPlanPrompt(goal: string): string {
  const normalized = goal.replace(/\r\n?/g, '\n').trim();
  const formatted = normalized.includes('\n') ? normalized : normalized.replace(/\s+/g, ' ');
  const bounded = formatted.slice(0, PLAN_PROMPT_MAX_GOAL);
  const goalBlock = [
    bounded ? `Goal:${bounded.includes('\n') ? '\n' : ' '}${bounded}` : 'Goal: ask the user for the goal',
    formatted.length > PLAN_PROMPT_MAX_GOAL ? PLAN_PROMPT_TRUNCATION_MARKER : '',
  ].filter(Boolean).join('\n');
  return `[PLAN MODE] Build a reviewable plan collaboratively, then ask once whether to Start implementation.
${goalBlock}

1. Check the request and repository first. Keep research proportional to the decision.
2. Use the askUser widget only when a decision-changing question remains; otherwise continue without an interview.
3. Show the user “Creating plan…” while preparing the review.
4. Create or update a reviewable RFC under .octocode/rfc/, then call plan with queries:[{reasoning:"Propose the reviewed plan.", action:"propose", rfcPath, steps}] using dependency-ordered, verifiable steps.
5. The plan tool owns one decision: Start implementation or Request changes, and presents the concise overview.
6. Choosing Start approves the exact RFC revision and begins implementation in one action. If interaction is unavailable, leave the plan pending; never infer approval from prose. There is no separate Accept step.

Planning does not disable tools. Use the tools needed to research and author the RFC, but do not implement the proposed source changes before the user chooses Start.`;
}
