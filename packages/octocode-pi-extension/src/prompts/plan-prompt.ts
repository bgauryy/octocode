import { buildPlanPrompt as buildSharedPlanPrompt } from '../contracts/prompts/index.js';

/** Pi supplies exact tool syntax; agent-contracts owns the planning and authorization semantics. */
export function buildPlanPrompt(goal: string): string {
  return buildSharedPlanPrompt(goal, {
    proposalInstruction:
      'Call plan with queries:[{reasoning:"Propose the reviewed plan.", action:"propose", steps, consequential, reason, rfcPath?}]. The tool shows “Creating plan…” and presents the overview and askUser-backed review.',
    reviewInstruction:
      'Use the returned decision: approved and started means continue implementation; requested changes mean revise; pending means wait for the existing interaction. The plan tool owns the review: do not call askUser again or repeat its progress, steps, or links. Present an inline review only if the tool explicitly reports that interactive review is unavailable.',
  });
}
