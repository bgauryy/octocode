import { createHash, randomUUID } from 'node:crypto';
import type { Plan, PlanGraphResult, SourceStep, Task } from '@octocodeai/agent-contracts/entities';
import { beginWrite } from '../db-transaction.js';
import { createPlan, getPlan, updatePlanStatus } from '../plans.js';
import { getTask, normalizeTaskPaths } from '../tasks-catalog.js';
import { addTaskDependency, createTask, listTasks } from '../tasks-ready.js';
import { normalizeWorkspacePath } from '../git.js';
import { AwarenessSchemaHelpers } from './coordination-schema-helpers.js';
import { planEntity, taskEntity } from './canonical-plan-task-entities.js';

function required(value: string | null | undefined, field: string): string {
  const normalized = value?.trim();
  if (!normalized) throw new Error(`${field} is required`);
  return normalized;
}

function canonicalWorkspace(workspace: string): string {
  return normalizeWorkspacePath(workspace, workspace) ?? workspace;
}

function projectionSteps(steps: SourceStep[]): Array<Required<Pick<SourceStep, 'sourceStepKey' | 'title'>> & SourceStep> {
  const keys = new Set<string>();
  return steps.map((step) => {
    const sourceStepKey = required(step.sourceStepKey, 'step.sourceStepKey');
    if (keys.has(sourceStepKey)) throw new Error(`materializePlanGraph: duplicate source step key: ${sourceStepKey}`);
    keys.add(sourceStepKey);
    if (!step.paths?.length) throw new Error(`materializePlanGraph: paths are required for ${sourceStepKey}`);
    required(step.reasoning, `reasoning for ${sourceStepKey}`);
    required(step.acceptance, `acceptance for ${sourceStepKey}`);
    return { ...step, sourceStepKey, title: required(step.title, 'step.title') };
  });
}

function projectionDigest(params: { title: string; goal?: string | null; rfcPath?: string | null; rfcRevision?: string | null; steps: SourceStep[] }): string {
  return createHash('sha256').update(JSON.stringify([
    params.title.trim(), params.goal?.trim() || null, params.rfcPath?.trim() || null, params.rfcRevision?.trim() || null,
    params.steps.map((step) => [step.sourceStepKey, step.title, step.paths, step.reasoning, step.acceptance, step.checkCommand, step.priority, [...new Set(step.dependsOnStepKeys ?? [])].sort()])
      .sort((left, right) => String(left[0]).localeCompare(String(right[0]))),
  ])).digest('hex');
}

export abstract class CoordinationPlanGraph extends AwarenessSchemaHelpers {
  getPlanBySourceKey(params: { sourceKind: string; sourceKey: string }): Plan | null {
    const sourceKind = params.sourceKind?.trim();
    const sourceKey = params.sourceKey?.trim();
    if (!sourceKind || !sourceKey) return null;
    const row = this.db.prepare(`SELECT * FROM awareness_plans
      WHERE workspace_path = ? AND source_kind = ? AND source_key = ?`)
      .get(canonicalWorkspace(this.workspace), sourceKind, sourceKey) as Record<string, unknown> | undefined;
    return row ? planEntity(row as never) : null;
  }

  reconcilePlanGraph(params: { planId: string }): Map<string, Task> {
    this.getPlan(params.planId);
    const tasks = listTasks(this.db, { planId: params.planId, workspacePath: canonicalWorkspace(this.workspace) });
    return new Map(tasks.filter((task) => task.source_step_key).map((task) => [task.source_step_key!, taskEntity(task)]));
  }

  abandonPlan(params: { planId: string; agentId: string; reason?: string | null }): { plan: Plan; cancelled: number } {
    const before = listTasks(this.db, { planId: params.planId, workspacePath: canonicalWorkspace(this.workspace) })
      .filter((task) => !['DONE', 'CANCELLED'].includes(task.status)).length;
    const plan = updatePlanStatus(this.db, { planId: params.planId, status: 'CANCELLED', agentId: required(params.agentId, 'agent-id') });
    return { plan: planEntity(plan), cancelled: before };
  }

  materializePlanGraph(params: {
    sourcePlanKey: string; sourceKind?: string | null; title: string; goal?: string | null;
    rfcPath?: string | null; rfcRevision?: string | null; agentId: string; steps: SourceStep[];
  }): PlanGraphResult {
    const sourceKind = params.sourceKind?.trim() || 'local';
    const workspace = canonicalWorkspace(this.workspace);
    const sourceKey = required(params.sourcePlanKey, 'sourcePlanKey');
    const title = required(params.title, 'title');
    const agentId = required(params.agentId, 'agent-id');
    const steps = projectionSteps(params.steps);
    const revision = params.rfcRevision?.trim() || null;
    const digest = projectionDigest({ ...params, title, steps });
    const transaction = beginWrite(this.db);
    try {
      let plan = this.db.prepare(`SELECT * FROM awareness_plans
        WHERE workspace_path = ? AND source_kind = ? AND source_key = ?`)
        .get(workspace, sourceKind, sourceKey) as Record<string, unknown> | undefined;
      if (plan && String(plan['rfc_revision'] ?? '') !== String(revision ?? '')) {
        throw new Error(`materializePlanGraph: projection revision conflict for ${sourceKey}`);
      }
      if (!plan) {
        const created = createPlan(this.db, {
          name: title,
          objective: required(params.goal, 'goal'),
          leadAgentId: agentId,
          workspacePath: workspace,
        }).plan;
        this.db.prepare(`UPDATE awareness_plans SET source_kind = ?, source_key = ?, rfc_path = ?, rfc_revision = ?
          WHERE plan_id = ?`).run(sourceKind, sourceKey, params.rfcPath?.trim() || null, revision, created.plan_id);
        plan = getPlan(this.db, created.plan_id)! as unknown as Record<string, unknown>;
      } else {
        if (plan['status'] !== 'ACTIVE') throw new Error(`materializePlanGraph: source plan is ${String(plan['status'])}: ${sourceKey}`);
        this.db.prepare(`UPDATE awareness_plans SET name = ?, objective = ?, rfc_path = ?, updated_at = ? WHERE plan_id = ?`)
          .run(title, required(params.goal, 'goal'), params.rfcPath?.trim() || null, new Date().toISOString(), String(plan['plan_id']));
        plan = getPlan(this.db, String(plan['plan_id']))! as unknown as Record<string, unknown>;
      }
      const planId = String(plan['plan_id']);
      const taskIds = new Map<string, string>();
      for (const step of steps) {
        const existing = this.db.prepare('SELECT task_id FROM awareness_tasks WHERE plan_id = ? AND source_step_key = ?')
          .get(planId, step.sourceStepKey) as { task_id: string } | undefined;
        if (!existing) {
          const task = createTask(this.db, {
            planId,
            title: step.title,
            paths: step.paths!,
            reasoning: required(step.reasoning, 'reasoning'),
            acceptanceCriteria: required(step.acceptance, 'acceptance'),
            createdBy: agentId,
            priority: step.priority,
          }).task;
          this.db.prepare('UPDATE awareness_tasks SET source_step_key = ?, check_command = ? WHERE task_id = ?')
            .run(step.sourceStepKey, step.checkCommand?.trim() || null, task.task_id);
          taskIds.set(step.sourceStepKey, task.task_id);
          continue;
        }
        const task = getTask(this.db, existing.task_id)!;
        const unchanged = JSON.stringify([task.title, task.paths, task.reasoning, task.acceptance_criteria, task.check_command]) === JSON.stringify([
          step.title, step.paths, step.reasoning?.trim(), step.acceptance?.trim(), step.checkCommand?.trim() || null,
        ]);
        if (task.status !== 'OPEN' && !unchanged) throw new Error(`cannot change ${task.status} task contract: ${task.task_id}`);
        if (task.status === 'OPEN' && !unchanged) {
          this.db.prepare(`UPDATE awareness_tasks SET title = ?, reasoning = ?, acceptance_criteria = ?, check_command = ?, priority = ?, updated_at = ?
            WHERE task_id = ?`).run(step.title, step.reasoning!.trim(), step.acceptance!.trim(), step.checkCommand?.trim() || null, step.priority ?? 0, new Date().toISOString(), task.task_id);
          const paths = normalizeTaskPaths(workspace, step.paths!);
          this.db.prepare('DELETE FROM task_paths WHERE task_id = ?').run(task.task_id);
          const insertPath = this.db.prepare('INSERT INTO task_paths(task_id, path, ordinal) VALUES (?, ?, ?)');
          paths.forEach((path, ordinal) => insertPath.run(task.task_id, path, ordinal));
        }
        taskIds.set(step.sourceStepKey, task.task_id);
      }
      for (const step of steps) {
        const taskId = taskIds.get(step.sourceStepKey)!;
        const dependencyIds = (step.dependsOnStepKeys ?? []).map((key) => {
          const dependencyId = taskIds.get(key);
          if (!dependencyId) throw new Error(`materializePlanGraph: dependency step key not in graph: ${key}`);
          return dependencyId;
        });
        const task = getTask(this.db, taskId)!;
        if (task.status !== 'OPEN' && JSON.stringify([...task.dependencies].sort()) !== JSON.stringify([...dependencyIds].sort())) {
          throw new Error(`cannot change ${task.status} task dependencies: ${taskId}`);
        }
        if (task.status === 'OPEN') {
          this.db.prepare('DELETE FROM task_dependencies WHERE task_id = ?').run(taskId);
          dependencyIds.forEach((dependsOnTaskId) => addTaskDependency(this.db, { taskId, dependsOnTaskId, agentId }));
        }
      }
      const latest = this.db.prepare(`SELECT event_id, created_at, payload_json FROM event_outbox
        WHERE workspace_path = ? AND aggregate_kind = 'plan' AND aggregate_id = ? AND event_type = 'plan.projected'
        ORDER BY sequence DESC LIMIT 1`).get(this.workspace, planId) as { event_id: string; created_at: string; payload_json: string } | undefined;
      const prior = latest && (JSON.parse(latest.payload_json) as { projectionDigest?: string }).projectionDigest === digest ? latest : undefined;
      this.insertOutboxEvent({
        version: 1,
        eventId: prior?.event_id ?? `evt_${randomUUID().replace(/-/g, '')}`,
        workspace,
        type: 'plan.projected',
        actor: { kind: 'system', id: 'awareness-plan-projector' },
        provenance: { source: 'harness', trust: 'authority' },
        aggregate: { kind: 'plan', id: planId, ...(revision ? { revision } : {}) },
        createdAt: prior?.created_at ?? new Date().toISOString(),
        payload: { sourceKind, sourcePlanKey: sourceKey, stepKeys: [...taskIds.keys()].sort(), projectionDigest: digest },
      });
      transaction.commit();
      const tasks = new Map<string, Task>();
      for (const [stepKey, taskId] of taskIds) tasks.set(stepKey, taskEntity(getTask(this.db, taskId)!));
      return { plan: this.getPlan(planId), tasks };
    } catch (error) {
      transaction.rollback();
      throw error;
    }
  }
}
