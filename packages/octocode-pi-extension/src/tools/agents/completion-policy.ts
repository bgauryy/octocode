export type WorkerCompletionAction = 'continue' | 'wrap-up' | 'partial-handback';

export interface WorkerCompletionDecision {
  action: WorkerCompletionAction;
  hard: boolean;
  remainingSteps: number;
  message?: string;
}

/** Pure policy: warn before the circuit breaker, then demand an honest partial handback. */
export function evaluateWorkerCompletionPolicy(input: {
  completedSteps: number;
  maxSteps: number;
  hasTerminalHandback?: boolean;
}): WorkerCompletionDecision {
  const completedSteps = Math.max(0, Math.floor(input.completedSteps));
  const maxSteps = Math.max(0, Math.floor(input.maxSteps));
  const remainingSteps = Math.max(0, maxSteps - completedSteps);
  if (input.hasTerminalHandback || maxSteps === 0) {
    return { action: 'continue', hard: false, remainingSteps };
  }
  if (completedSteps >= maxSteps) {
    return {
      action: 'partial-handback',
      hard: true,
      remainingSteps,
      message: `Worker reached the ${maxSteps}-step budget. Return a partial handback now with status, result, evidence, verification, confidence, and next action; do not imply completion.`,
    };
  }
  if (completedSteps >= Math.ceil(maxSteps * 0.8)) {
    return {
      action: 'wrap-up',
      hard: false,
      remainingSteps,
      message: `Worker has ${remainingSteps} step${remainingSteps === 1 ? '' : 's'} left before its ${maxSteps}-step budget. Wrap up the smallest verified result and prepare the typed handback.`,
    };
  }
  return { action: 'continue', hard: false, remainingSteps };
}
