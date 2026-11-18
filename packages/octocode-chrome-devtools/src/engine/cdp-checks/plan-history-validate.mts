// Plan-time checks for `history` and `reload` steps.
import { chromeHistoryDirections } from '../chrome-contract.mjs';

export function validateHistory(step: {
  op: string;
  direction?: unknown;
  ignoreCache?: unknown;
}) {
  if (
    step.op === 'history' &&
    !(chromeHistoryDirections as readonly unknown[]).includes(step.direction)
  )
    throw new Error(
      `history needs direction ${chromeHistoryDirections.join(' or ')}`
    );
  if (step.ignoreCache !== undefined && typeof step.ignoreCache !== 'boolean')
    throw new Error('ignoreCache must be boolean');
}
