/** Host-neutral no-mutation contract used when an Octocode plan starts. */

export const PLAN_PROMPT_MAX_GOAL = 2000;
export const PLAN_PROMPT_TRUNCATION_MARKER =
  `[Goal truncated at ${PLAN_PROMPT_MAX_GOAL} characters; ask the user to restate omitted constraints before proposing.]`;

/** Build a bounded planning request; the plan tool and RFC skill own field-level detail. */
export function buildPlanPrompt(goal: string): string {
  const normalized = goal.replace(/\r\n?/g, '\n').trim();
  const formatted = normalized.includes('\n') ? normalized : normalized.replace(/\s+/g, ' ');
  const truncated = formatted.length > PLAN_PROMPT_MAX_GOAL;
  const clean = formatted.slice(0, PLAN_PROMPT_MAX_GOAL);
  const renderedGoal = clean.includes('\n') ? `Goal:\n${clean}` : `Goal: ${clean}`;
  const target = clean
    ? `${renderedGoal}${truncated ? `\n${PLAN_PROMPT_TRUNCATION_MARKER}` : ''}`
    : 'Goal: (ask the user for the goal before planning)';
  return [
    '[PLAN MODE] Do not change files or run mutating commands until the required authorization gate is complete.',
    target,
    '',
    '1. Establish only the evidence that changes scope, dependencies, risk, or acceptance. For a simple request, keep this brief; for shared or cross-cutting work, trace the relevant callers and contracts.',
    '2. If the work is consequential, use the RFC workflow, discuss the document with the user, and wait for acceptance. Otherwise state why a lightweight plan is sufficient.',
    '3. Ask bounded clarification only for material choices the repository cannot answer. Do not turn reversible implementation details into questions.',
    '4. Call plan(propose) with dependency-ordered, independently verifiable steps and a final real-world check. A consequential proposal must link its reviewable RFC; show the terminal Summary first, then ask whether to open the full browser review.',
    '5. For a lightweight plan, approval authorizes execution. For a consequential RFC, every review surface shows the same revision and local files; acceptance binds that revision but does not authorize implementation. Require a separate user Start after rechecking the accepted bytes. Feedback means revise and re-propose; rejection means stop; never execute a rejected plan.',
    '',
    'Return the plan result, main risk, and excluded scope. Do not change code before approval, and for consequential RFC work do not change code before the separate Start.',
  ].join('\n');
}
