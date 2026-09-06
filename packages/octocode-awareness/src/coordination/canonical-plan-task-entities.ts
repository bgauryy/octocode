import type { Plan, PlanRecord, PlanTaskRecord, Task } from '@octocodeai/octocode-shared/entities';

export function planEntity(plan: PlanRecord): Plan {
  return {
    planId: plan.plan_id,
    title: plan.name,
    goal: plan.objective,
    status: plan.status,
    createdAt: plan.created_at,
    updatedAt: plan.updated_at,
    sourceKind: plan.source_kind,
    sourceKey: plan.source_key,
    rfcPath: plan.rfc_path,
    rfcRevision: plan.rfc_revision,
  };
}

export function taskEntity(task: PlanTaskRecord): Task {
  return {
    taskId: task.task_id,
    planId: task.plan_id,
    title: task.title,
    filePath: task.paths[0] ?? null,
    paths: task.paths,
    reasoning: task.reasoning,
    acceptance: task.acceptance_criteria,
    checkCommand: task.check_command,
    status: task.status,
    priority: task.priority,
    dependencies: task.dependencies,
    agentId: task.claim?.agent_id ?? null,
    runId: task.claim?.run_id ?? task.run_id,
    claimedAt: task.claim?.claimed_at ?? null,
    leaseExpiresAt: task.claim?.expires_at ?? null,
    createdAt: task.created_at,
    updatedAt: task.updated_at,
    doneAt: task.completed_at,
    verifiedAt: task.run_status === 'SUCCESS' ? task.verification?.created_at ?? null : null,
    verifiedBy: task.verification?.agent_id ?? null,
    verificationMessage: task.verification?.message ?? null,
    sourceStepKey: task.source_step_key,
  };
}
