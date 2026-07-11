/**
 * OQL planner — the explain-plan shapes describing how a query's predicates
 * were routed to backends (pushdown/residual/route/unsupported), the backend
 * calls made, and transformer traces, surfaced when `explain:true`.
 */

import type { PredicateId, QuerySource } from './predicates.js';
import type {
  MaterializePolicy,
  OqlCanonicalInput,
  QueryControls,
} from './query.js';
import type { OqlDiagnostic } from './diagnostics.js';
import type { OqlContinuation } from './envelope.js';

export type PlanRoute = 'PUSHDOWN' | 'RESIDUAL' | 'ROUTE' | 'UNSUPPORTED';

export interface OqlPlanNode {
  predicateId: PredicateId;
  path: string;
  route: PlanRoute;
  backend?: string;
  reason: string;
}

export interface OqlBackendCall {
  backend: string;
  source?: QuerySource;
  operation: string;
  exact: boolean;
}

export interface OqlTransformerTrace {
  id: string;
  status: string;
  sourceKinds: readonly string[];
  target: string;
  backends: readonly Pick<OqlBackendCall, 'backend' | 'operation' | 'exact'>[];
}

export interface OqlExplainPlan {
  input: unknown;
  normalized: OqlCanonicalInput;
  defaults: Record<string, unknown>;
  nodes: OqlPlanNode[];
  backendCalls: OqlBackendCall[];
  transformers?: readonly OqlTransformerTrace[];
  materialization?: MaterializePolicy & { required: boolean; reason: string };
  budgets: QueryControls['budget'];
  truncated?: boolean;
  diagnostics: OqlDiagnostic[];
  next?: Record<string, OqlContinuation>;
}
