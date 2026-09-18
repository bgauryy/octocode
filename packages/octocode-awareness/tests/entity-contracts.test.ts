import { afterEach, describe, expect, it } from 'vitest';
import { DatabaseSync } from '../src/sqlite.js';
import { PLAN_STATUSES, TASK_STATUSES, AGENT_STATUSES, PLAN_MEMBER_ROLES, PLAN_DOC_KINDS, TASK_RUN_ORIGINS, TASK_RUN_STATUSES } from '../src/entities.js';
import { initDb } from '../src/db-init.js';
import { schemas } from '../src/schema/registry.js';

const databases: DatabaseSync[] = [];
afterEach(() => databases.splice(0).forEach(db => db.close()));

describe('shared entities, request schemas, and database enums', () => {
  it.each([
    ['awareness_plans', 'status', PLAN_STATUSES, { plan_id: 'p', name: 'Plan', objective: 'Goal', lead_agent_id: 'a', workspace_path: '/repo', doc_dir: '/repo', created_at: 'now', updated_at: 'now' }],
    ['awareness_tasks', 'status', TASK_STATUSES, { task_id: 't', plan_id: 'p', title: 'Task', reasoning: 'Why', acceptance_criteria: 'Done', created_by: 'a', created_at: 'now', updated_at: 'now' }],
    ['awareness_agents', 'status', AGENT_STATUSES, { agent_id: 'a', registered_at: 'now', last_seen_at: 'now' }],
    ['plan_members', 'role', PLAN_MEMBER_ROLES, { plan_id: 'p', agent_id: 'a', joined_at: 'now' }],
    ['plan_docs', 'kind', PLAN_DOC_KINDS, { plan_id: 'p', relative_path: 'plan.md', title: 'Plan' }],
    ['task_runs', 'origin', TASK_RUN_ORIGINS, { run_id: 'r', agent_id: 'a', rationale: 'Why', test_plan: 'Check' }],
    ['task_runs', 'status', TASK_RUN_STATUSES, { run_id: 'r', agent_id: 'a', rationale: 'Why', test_plan: 'Check' }],
  ] as const)('keeps %s.%s aligned with its entity contract', (table, column, values, seed) => {
    const db = new DatabaseSync(':memory:');
    databases.push(db);
    initDb(db);
    db.exec("INSERT INTO awareness_plans(plan_id,name,objective,lead_agent_id,workspace_path,doc_dir,created_at,updated_at) VALUES('p','Plan','Goal','a','/repo','/repo','now','now')");
    if (table === 'awareness_plans') db.exec('DELETE FROM awareness_plans');
    const fields = Object.keys(seed);
    const statement = db.prepare(`INSERT INTO ${table}(${[...fields, column].join(',')}) VALUES(${[...fields, column].map(() => '?').join(',')})`);
    for (const value of [...values, 'LEGACY', values[0].toLowerCase()]) {
      db.exec('SAVEPOINT enum_probe');
      try {
        const write = () => statement.run(...Object.values(seed), value);
        if ((values as readonly string[]).includes(value)) expect(write).not.toThrow();
        else expect(write).toThrow(/CHECK constraint/);
      } finally { db.exec('ROLLBACK TO enum_probe; RELEASE enum_probe'); }
    }
  });

  it('publishes and validates only canonical plan and task states', () => {
    for (const value of [...PLAN_STATUSES, 'DONE', 'LEGACY']) {
      expect(schemas.plan.safeParse({ action: 'list', status: value }).success).toBe((PLAN_STATUSES as readonly string[]).includes(value));
    }
    for (const value of [...TASK_STATUSES, 'COMPLETED', 'LEGACY']) {
      expect(schemas.task.safeParse({ action: 'list', status: value }).success).toBe((TASK_STATUSES as readonly string[]).includes(value));
    }
  });
});
