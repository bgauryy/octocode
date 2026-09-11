import { describe, expect, it } from 'vitest';
import { evaluateWorkerCompletionPolicy } from '../src/tools/agents/completion-policy.js';

describe('worker completion policy', () => {
  it('requests a soft wrap-up at 80% of the step budget', () => {
    expect(evaluateWorkerCompletionPolicy({ completedSteps: 48, maxSteps: 60 })).toMatchObject({
      action: 'wrap-up',
      hard: false,
      remainingSteps: 12,
    });
  });

  it('requires a partial handback at the hard budget instead of pretending completion', () => {
    const result = evaluateWorkerCompletionPolicy({ completedSteps: 60, maxSteps: 60 });
    expect(result).toMatchObject({ action: 'partial-handback', hard: true, remainingSteps: 0 });
    expect(result.message).toMatch(/evidence.*verification.*next/i);
  });

  it('does not nag after a terminal handback or below the wrap-up threshold', () => {
    expect(evaluateWorkerCompletionPolicy({ completedSteps: 47, maxSteps: 60 }).action).toBe('continue');
    expect(evaluateWorkerCompletionPolicy({ completedSteps: 60, maxSteps: 60, hasTerminalHandback: true }).action).toBe('continue');
  });
});
