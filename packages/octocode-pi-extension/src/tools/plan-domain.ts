export const PLAN_DOMAIN_PHASES = [
  'researching', 'needs_answers', 'draft', 'in_review', 'accepted',
  'executing', 'verifying', 'complete', 'blocked', 'failed', 'abandoned',
] as const;

export type PlanPhase = typeof PLAN_DOMAIN_PHASES[number];
export type PlanCommand =
  | 'research' | 'request_answers' | 'draft' | 'review' | 'accept' | 'start'
  | 'execute' | 'verify' | 'complete' | 'block' | 'fail' | 'abandon' | 'revise'
  | 'compensate_start_failure';

export interface PlanTransitionEventV1 {
  version: 1;
  type: 'plan.phase.changed';
  from: PlanPhase;
  to: PlanPhase;
  command: PlanCommand;
}

const TARGET: Readonly<Record<PlanCommand, PlanPhase>> = Object.freeze({
  research: 'researching', request_answers: 'needs_answers', draft: 'draft', review: 'in_review',
  accept: 'accepted', start: 'executing', execute: 'executing', verify: 'verifying', complete: 'complete',
  block: 'blocked', fail: 'failed', abandon: 'abandoned', revise: 'draft',
  compensate_start_failure: 'accepted',
});

const ALLOWED: Readonly<Record<PlanPhase, readonly PlanCommand[]>> = Object.freeze({
  abandoned: ['research', 'request_answers'],
  researching: ['request_answers', 'draft', 'block', 'fail', 'abandon'],
  needs_answers: ['research', 'draft', 'block', 'fail', 'abandon'],
  draft: ['research', 'request_answers', 'review', 'block', 'fail', 'abandon'],
  in_review: ['request_answers', 'accept', 'revise', 'block', 'fail', 'abandon'],
  accepted: ['start', 'revise', 'block', 'fail', 'abandon'],
  executing: ['execute', 'verify', 'block', 'fail', 'abandon', 'compensate_start_failure'],
  verifying: ['execute', 'complete', 'block', 'fail', 'abandon'],
  blocked: ['research', 'request_answers', 'draft', 'review', 'execute', 'verify', 'fail', 'abandon'],
  failed: ['research', 'abandon'],
  complete: ['research'],
});

export function canTransitionPlan(from: PlanPhase, command: PlanCommand): boolean {
  return ALLOWED[from].includes(command);
}

export function transitionPlan(from: PlanPhase, command: PlanCommand): PlanTransitionEventV1 {
  if (!canTransitionPlan(from, command)) throw new Error(`Invalid plan transition: ${from} --${command}--> ${TARGET[command]}`);
  return { version: 1, type: 'plan.phase.changed', from, to: TARGET[command], command };
}

export function transitionPlanTo(from: PlanPhase, to: PlanPhase): PlanTransitionEventV1 | undefined {
  if (from === to) return undefined;
  // Compensation is intentionally unavailable through the generic phase API:
  // callers must explicitly name a failed Start projection rollback.
  const candidates = (Object.keys(TARGET) as PlanCommand[]).filter((command) =>
    command !== 'compensate_start_failure' && TARGET[command] === to && canTransitionPlan(from, command));
  if (candidates.length === 0) throw new Error(`Invalid plan transition: ${from} --> ${to}`);
  return transitionPlan(from, candidates[0]!);
}
