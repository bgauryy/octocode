import { describe, expect, it } from 'vitest';
import { PLAN_DOMAIN_PHASES, canTransitionPlan, transitionPlan, transitionPlanTo } from '../src/tools/plan-domain.js';

describe('PlanDomain', () => {
  it('implements the review and execution happy path', () => {
    let phase = transitionPlan('abandoned', 'research').to;
    phase = transitionPlan(phase, 'draft').to;
    phase = transitionPlan(phase, 'review').to;
    phase = transitionPlan(phase, 'accept').to;
    phase = transitionPlan(phase, 'start').to;
    phase = transitionPlan(phase, 'verify').to;
    phase = transitionPlan(phase, 'complete').to;
    expect(phase).toBe('complete');
  });

  it('rejects review bypasses and terminal mutation', () => {
    expect(() => transitionPlan('draft', 'start')).toThrow(/Invalid plan transition/);
    expect(() => transitionPlanTo('complete', 'executing')).toThrow(/Invalid plan transition/);
  });

  it('exposes failed Start compensation only through its explicit command', () => {
    expect(transitionPlan('executing', 'compensate_start_failure')).toMatchObject({
      from: 'executing', to: 'accepted', command: 'compensate_start_failure',
    });
    expect(() => transitionPlanTo('executing', 'accepted')).toThrow(/Invalid plan transition/);
    expect(() => transitionPlan('accepted', 'compensate_start_failure')).toThrow(/Invalid plan transition/);
  });

  it('has an explicit transition policy for every phase', () => {
    for (const phase of PLAN_DOMAIN_PHASES) {
      expect(['research', 'request_answers', 'draft', 'review', 'accept', 'start', 'execute', 'verify', 'complete', 'block', 'fail', 'abandon', 'revise', 'compensate_start_failure']
        .some((command) => canTransitionPlan(phase, command as never))).toBe(true);
    }
  });
});
