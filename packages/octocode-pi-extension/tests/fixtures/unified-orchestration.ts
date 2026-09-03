export const RETRY_AFTER_SHARED_COMMIT_FIXTURE = {
  sourcePlanKey: 'octocode-plan:session-1:branch-main:revision-abc',
  sourceStepKeys: ['step-schema', 'step-runtime'],
  injectedFailure: 'local-mapping-persist-after-awareness-commit',
  expected: {
    awarenessPlanCount: 1,
    awarenessTaskCount: 2,
    preservePlanIdOnRetry: true,
    preserveTaskIdsOnRetry: true,
  },
} as const;

export const FORKED_SESSION_FIXTURE = {
  parent: {
    lifecycle: 'executing',
    awarenessPlanId: 'plan_parent',
    awarenessTaskIds: ['task_parent_schema', 'task_parent_runtime'],
  },
  fork: {
    lifecycle: 'accepted',
    awarenessPlanId: undefined,
    awarenessTaskIds: [],
    requiresExplicitStart: true,
  },
} as const;

export const TASK_LINKED_WORKER_TERMINAL_FIXTURES = [
  { terminal: 'done', taskStatus: 'DONE', verificationDebt: true, releaseOwnership: false },
  { terminal: 'blocked', taskStatus: 'OPEN', verificationDebt: false, releaseOwnership: true },
  { terminal: 'failed', taskStatus: 'OPEN', verificationDebt: false, releaseOwnership: true },
  { terminal: 'aborted', taskStatus: 'OPEN', verificationDebt: false, releaseOwnership: true },
  { terminal: 'killed', taskStatus: 'OPEN', verificationDebt: false, releaseOwnership: true },
  { terminal: 'shutdown', taskStatus: 'OPEN', verificationDebt: false, releaseOwnership: true },
] as const;
