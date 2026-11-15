import { errorMessage } from '../error-message.mjs';
import type { LocatedStep, StepDetails } from './dom-types.mjs';

export function actionabilityReason(d: StepDetails): string {
  return (
    [
      !d.visible &&
        `not visible (display=${d.style?.display}, visibility=${d.style?.visibility}, opacity=${d.style?.opacity}, box=${JSON.stringify(d.bbox)})`,
      d.disabled && 'disabled',
      d.covered && `covered by ${d.coveredBy}`,
      d.stable === false && 'still moving',
      d.style?.pointerEvents === 'none' && 'pointer-events:none',
    ]
      .filter(Boolean)
      .join('; ') || 'actionability check failed'
  );
}

// Retain the last completed observation when the next check loses its acknowledgement.
// It is evidence from before the failure, never proof of the current page or dispatch outcome.
export function observedDomFailure(error: unknown, observed: LocatedStep) {
  const failure =
    error instanceof Error ? error : new Error(errorMessage(error));
  const { label, details } = observed;
  const reason =
    details.operation === 'blocked-by-actionability'
      ? actionabilityReason(details)
      : details.error ||
        `found=${Boolean(details.found)} visible=${Boolean(details.visible)}`;
  failure.message += `; last observed ${label}: ${reason}. Inspect current state before retrying.`;
  return Object.assign(failure, {
    domObservation: { label, ...details, lastObserved: true },
  });
}
