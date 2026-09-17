import { buildPlanPrompt as buildSharedPlanPrompt } from '../contracts/prompts/index.js';

/** The Pi-local contract owns planning semantics; this adapter supplies exact tool syntax. */
export function buildPlanPrompt(goal: string): string {
  return buildSharedPlanPrompt(goal, {
    proposalInstruction:
      'Call plan with queries:[{action:"propose", steps, consequential, reason, rfcPath?}]. The tool shows “Creating plan…” and presents the overview and askUser-backed review.',
    reviewInstruction:
      'Use the returned decision: approved and started means continue implementation; requested changes mean revise; pending means wait for the existing interaction. The plan tool owns the review: do not call askUser again or repeat its progress, steps, or links. Present an inline review only if the tool explicitly reports that interactive review is unavailable.',
  });
}
