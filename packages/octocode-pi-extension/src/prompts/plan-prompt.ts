import {
  PLAN_PROMPT_MAX_GOAL,
  PLAN_PROMPT_TRUNCATION_MARKER,
  buildPlanPrompt as buildSharedPlanPrompt,
} from '@octocodeai/octocode-shared/prompts';

export { PLAN_PROMPT_MAX_GOAL, PLAN_PROMPT_TRUNCATION_MARKER };

export function buildPlanPrompt(goal: string): string {
  const shared = buildSharedPlanPrompt(goal);
  const goalBlock = shared.match(/(?:^|\n)(Goal:[\s\S]*?)(?=\n\n1\.)/)?.[1] ?? 'Goal: ask the user for the goal';
  return `[PLAN MODE] Build a reviewable plan collaboratively, then ask once whether to Start implementation.
${goalBlock}

1. Check the request and repository first. Keep research proportional to the decision.
2. Use the askUser widget only when a decision-changing question remains; otherwise continue without an interview.
3. Show the user “Creating plan…” while preparing the review.
4. Create or update a reviewable RFC under .octocode/rfc/, then call plan with queries:[{reasoning:"Propose the reviewed plan.", action:"propose", rfcPath, steps}] using dependency-ordered, verifiable steps.
5. Present a concise plan overview in the message and ask one decision: Start implementation or Request changes.
6. Choosing Start approves the exact RFC revision and begins implementation in one action. There is no separate Accept step.

Planning does not disable tools. Use the tools needed to research and author the RFC, but do not implement the proposed source changes before the user chooses Start.`;
}
