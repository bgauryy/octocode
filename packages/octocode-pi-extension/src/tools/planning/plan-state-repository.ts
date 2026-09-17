import type { PlanPhase } from '../plan-domain.js';
import type { PlanCoordination, PlanDecision, PlanStep, ReviewState } from './plan-types.js';
import type { PlanScope } from './plan-scope.js';

export type StoredReviewMetadata = Omit<ReviewState, 'phase' | 'rfcPath' | 'decisions'>;

export class PlanStateRepository {
  readonly plans = new Map<PlanScope, PlanStep[]>();
  readonly lifecycle = new Map<PlanScope, PlanPhase>();
  readonly review = new Map<PlanScope, StoredReviewMetadata>();
  readonly rfc = new Map<PlanScope, string>();
  readonly decisions = new Map<PlanScope, PlanDecision[]>();
  readonly coordination = new Map<PlanScope, PlanCoordination>();
  readonly loaded = new Set<PlanScope>();
  readonly cleared = new Set<PlanScope>();
  readonly turnsSinceUpdate = new Map<PlanScope, number>();
  readonly persistenceFailures = new Map<PlanScope, string>();

  release(scope: PlanScope): void {
    this.plans.delete(scope);
    this.lifecycle.delete(scope);
    this.review.delete(scope);
    this.rfc.delete(scope);
    this.decisions.delete(scope);
    this.coordination.delete(scope);
    this.loaded.delete(scope);
    this.cleared.delete(scope);
    this.turnsSinceUpdate.delete(scope);
    this.persistenceFailures.delete(scope);
  }
}
