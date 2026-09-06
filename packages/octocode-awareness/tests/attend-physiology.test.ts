import { describe, expect, it } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { initDb } from '../src/db-init.js';
import { attendAwareness } from '../src/attend-query.js';
import type { AttendParams } from '../src/attend-model.js';
import { assessOperationalState } from '../src/attend-physiology.js';

describe('observed agent physiology', () => {
  it('rejects removed aliases instead of falling back to the current workspace', () => {
    const db = new DatabaseSync(':memory:');
    initDb(db);
    try {
      expect(() => attendAwareness(db, { workspace: '/wrong-workspace' } as unknown as AttendParams))
        .toThrow(/attendAwareness: unknown options: workspace/);
    } finally { db.close(); }
  });

  it('prioritizes measured pressures without mutating records or inventing sensor completeness', () => {
    const workboard = {
      Verify: [{ agent_id: 'worker', column_total: 3 }],
      FilesUnderWork: [
        { path: 'src/shared.ts', agents: ['worker', 'peer'], locked: true, lock_agent: 'peer' },
        { path: 'src/own.ts', agents: ['worker', 'worker'], locked: true, lock_agent: 'worker' },
        { path: 'src/unrelated.ts', agents: ['peer'], locked: true, lock_agent: 'peer' },
      ],
    };
    const before = JSON.stringify(workboard);
    const result = assessOperationalState({
      workboard, agentId: 'worker', workspacePath: '/repo', files: ['src/shared.ts', '/repo/src/own.ts'],
      recalled: 2, referenceWarnings: 1,
    });
    expect(result.operational_state.coordination).toEqual({ overlaps_observed: 1, locks_observed: 1 });
    expect(result.operational_state.coverage).toEqual({ bounded: true, omitted_rows: 2 });
    expect(result.operational_state.evidence).toEqual({ recalled: 2, reference_warnings: 1 });
    expect(result.regulation).toEqual({ advisory: true, actions: [
      'verify_owned_work', 'inspect_lock', 'inspect_overlap', 'revalidate_memory', 'narrow_read',
    ] });
    expect(JSON.stringify(workboard)).toBe(before);
    expect(assessOperationalState({
      workboard, agentId: '', workspacePath: '/repo', files: [], recalled: 0, referenceWarnings: 0,
    }).regulation.actions).toEqual(['narrow_read']);
  });

  it('keeps unknown runtime sensors explicit and empty orientation terminal', () => {
    const db = new DatabaseSync(':memory:');
    initDb(db);
    try {
      const compact = attendAwareness(db, { workspacePath: '/repo', agentId: 'worker', compact: true });
      const full = attendAwareness(db, { workspacePath: '/repo', agentId: 'worker' });
      expect(compact.operational_state).toEqual(full.operational_state);
      expect(compact.operational_state?.unavailable).toContain('context');
      expect(compact.regulation).toEqual({ advisory: true, actions: [] });
      expect(compact.next.action).toBe('continue');
      expect(compact.next.reason).toContain('authorized task');
      expect(Buffer.byteLength(JSON.stringify(compact))).toBeLessThan(2048);
      expect(attendAwareness(db, { workspacePath: '/repo', compact: true, explainOrgan: true }).organ_reference).toBeDefined();
    } finally { db.close(); }
  });

  it('regulates owned verification without promoting peer debt to an obligation', () => {
    const db = new DatabaseSync(':memory:');
    initDb(db);
    try {
      const now = new Date().toISOString();
      db.prepare(`INSERT INTO task_runs
        (run_id, origin, agent_id, rationale, test_plan, status, workspace_path, created_at, updated_at)
        VALUES ('run_observed', 'WORK', 'owner', 'bounded change', 'focused check', 'PENDING', '/repo', ?, ?)`)
        .run(now, now);
      const owner = attendAwareness(db, { workspacePath: '/repo', agentId: 'owner', compact: true });
      const peer = attendAwareness(db, { workspacePath: '/repo', agentId: 'peer', compact: true });
      expect(owner.operational_state?.verification).toEqual({ owned_observed: 1, total: 1 });
      expect(owner.regulation?.actions).toContain('verify_owned_work');
      expect(peer.regulation?.actions).not.toContain('verify_owned_work');
      expect(peer.operational_state?.verification).toEqual({ owned_observed: 0, total: 1 });
      expect(db.prepare('SELECT status FROM task_runs').get()).toEqual({ status: 'PENDING' });
    } finally { db.close(); }
  });

  it('selects the scoped peer observation before unrelated workspace presence', () => {
    const db = new DatabaseSync(':memory:');
    initDb(db);
    try {
      const now = new Date().toISOString();
      const future = new Date(Date.now() + 60_000).toISOString();
      for (const [id, path] of [['unrelated', '/repo/a.ts'], ['scoped', '/repo/z.ts']] as const) {
        db.prepare(`INSERT INTO task_runs
          (run_id, origin, agent_id, rationale, test_plan, status, workspace_path, created_at, updated_at)
          VALUES (?, 'WORK', 'peer', 'bounded change', 'focused check', 'ACTIVE', '/repo', ?, ?)`)
          .run(id, now, now);
        db.prepare(`INSERT INTO run_files (run_id, file_path, source, started_at, heartbeat_at, expires_at)
          VALUES (?, ?, 'EXPLICIT', ?, ?, ?)`).run(id, path, now, now, future);
      }
      const result = attendAwareness(db, { workspacePath: '/repo', agentId: 'worker', file: 'z.ts', compact: true });
      expect(result.regulation.actions).toContain('inspect_overlap');
      expect(result.next).toMatchObject({ action: 'inspect_overlap', target: { file: 'z.ts' } });
      const unrelated = attendAwareness(db, { workspacePath: '/repo', agentId: 'worker', file: 'untouched.ts', compact: true });
      expect(unrelated.regulation.actions).toEqual([]);
      expect(unrelated.next.action).toBe('continue');
    } finally { db.close(); }
  });
});


describe('host runtime observations', () => {
  it('reports measured runtime receipts and clears unavailable sensors without inventing budget', () => {
    const result = assessOperationalState({ workboard: {}, agentId: 'a', workspacePath: '/repo', files: [], recalled: 0, referenceWarnings: 0,
      runtimeObservation: { schema_version: 1, source: 'native_runtime', context: {
        current_tokens: 120, measured_at: 10, input_limit_tokens: 120,
        remaining_input_tokens: 0, saturation_basis_points: 10_000,
      },
        tools: { window: 32, observed: 3, failed: 2, cancelled: 0, blocked: 0 },
        controls: { owner: 'agent_core', compactions_committed: 1, compactions_failed: 0, retries_scheduled: 1, provider_attempt: 2, provider_max_attempts: 3 } },
    });
    expect(result.operational_state.runtime?.context?.current_tokens).toBe(120);
    expect(result.operational_state.unavailable).not.toContain('context');
    expect(result.operational_state.unavailable).not.toContain('tool_health');
    expect(result.operational_state.unavailable).toContain('budget');
    expect(result.regulation.actions).toContain('inspect_recent_tool_failures');
    expect(result.regulation.actions).toContain('inspect_context_headroom');
    expect(result.operational_state.runtime?.controls.compactions_committed).toBe(1);
  });

  it('rejects malformed host observations and never infers health from absent tool samples', () => {
    const base = { workboard: {}, agentId: 'a', workspacePath: '/repo', files: [], recalled: 0, referenceWarnings: 0 };
    expect(() => assessOperationalState({ ...base, runtimeObservation: { schema_version: 1, source: 'native_runtime',
      tools: { window: 32, observed: 1, failed: 2, cancelled: 0, blocked: 0 },
      controls: { owner: 'agent_core', compactions_committed: 0, compactions_failed: 0, retries_scheduled: 0 } } })).toThrow(/runtime observation/i);
    const empty = assessOperationalState({ ...base, runtimeObservation: { schema_version: 1, source: 'native_runtime',
      tools: { window: 32, observed: 0, failed: 0, cancelled: 0, blocked: 0 },
      controls: { owner: 'agent_core', compactions_committed: 0, compactions_failed: 0, retries_scheduled: 0 } } });
    expect(empty.operational_state.unavailable).toContain('context');
    expect(empty.operational_state.unavailable).toContain('tool_health');
    expect(empty.regulation.actions).toEqual([]);
  });

  it('advises from fresh measured headroom and clears pressure when the next measurement recovers', () => {
    const base = { workboard: {}, agentId: 'a', workspacePath: '/repo', files: [], recalled: 0, referenceWarnings: 0 };
    const observation = (current_tokens: number) => ({ schema_version: 1 as const, source: 'native_runtime' as const,
      context: {
        current_tokens, measured_at: 10, input_limit_tokens: 100,
        remaining_input_tokens: Math.max(0, 100 - current_tokens),
        saturation_basis_points: Math.min(10_000, Math.floor((current_tokens / 100) * 10_000)),
      },
      tools: { window: 32 as const, observed: 0, failed: 0, cancelled: 0, blocked: 0 },
      controls: { owner: 'agent_core' as const, compactions_committed: 0, compactions_failed: 0, retries_scheduled: 0 },
    });
    expect(assessOperationalState({ ...base, runtimeObservation: observation(90) }).regulation.actions)
      .toEqual(['inspect_context_headroom']);
    expect(assessOperationalState({ ...base, runtimeObservation: observation(20) }).regulation.actions).toEqual([]);
    expect(() => assessOperationalState({ ...base, runtimeObservation: {
      ...observation(90), context: { ...observation(90).context, remaining_input_tokens: 11 },
    } })).toThrow(/runtime observation/i);
  });
});
