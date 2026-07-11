/**
 * maintenance.test.ts — Behavioural tests for maintenance functions against the current schema.
 *
 * Core tables: memories, tasks, locks.
 * Core columns: importance, run_id, tags_json, memory_refs.
 */
import { describe, it, expect } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { randomUUID } from 'node:crypto';
import { initDb } from '../src/db.js';
import { insertMemory } from '../src/memory.js';
import { insertNotification } from '../src/notifications.js';
import { notifyGet } from '../src/maintenance.js';
// ─── Helpers ──────────────────────────────────────────────────────────────────
function freshDb(): DatabaseSync {
    const db = new DatabaseSync(':memory:');
    db.exec('PRAGMA foreign_keys = ON');
    initDb(db);
    return db;
}
/** Insert a memory using the memories table. */
function insertMem(db: DatabaseSync, opts: {
    memoryId?: string;
    importance?: number;
    label?: string;
    tags?: string[];
    failureSig?: string;
    observation?: string;
    workspacePath?: string | null;
} = {}): string {
    const memoryId = opts.memoryId ?? 'mem_' + randomUUID().replace(/-/g, '');
    const now = new Date().toISOString();
    db.prepare(`
    INSERT INTO memories (
      memory_id, agent_id, task_context, observation, importance,
      label, tags_json, workspace_path, failure_signature, created_at
    ) VALUES (?, 'agent-test', 'test context', ?, ?, ?, ?, ?, ?, ?)
  `).run(memoryId, opts.observation ?? 'test observation', opts.importance ?? 5, opts.label ?? 'OTHER', JSON.stringify(opts.tags ?? []), opts.workspacePath ?? null, opts.failureSig ?? null, now);
    return memoryId;
}

// ─── 3. notifyGet — reads from memories ──────────────────────────────────────

describe('notifyGet — smart briefing from memories table', () => {
  it('returns empty briefing when no memories exist', () => {
    const db = freshDb();
    const res = notifyGet(db, { agent_id: 'agent-a', workspace: '/ws' });
    expect(res.ok).toBe(true);
  });

  it('surfaces high-importance memories from memories table using importance column', () => {
    const db = freshDb();
    insertMem(db, {
      importance: 8,
      label: 'GOTCHA',
      observation: 'always check token expiry',
      workspacePath: '/ws',
    });

    const res = notifyGet(db, { agent_id: 'agent-b', workspace: '/ws' }) as {
      ok: true; count: number; notifications: Array<{ kind: string; text: string }>;
    };
    expect(res.ok).toBe(true);
    // The briefing should surface the GOTCHA memory
    expect(res.count).toBeGreaterThanOrEqual(1);
    expect(res.notifications.some(n => n.text.includes('GOTCHA'))).toBe(true);
  });

  it('selects one prompt-relevant memory for hook context and stays silent on unrelated memory', () => {
    const db = freshDb();
    insertMemory(db, {
      agentId: 'memory-agent',
      taskContext: 'release screenshots',
      importance: 10,
      label: 'GOTCHA',
      observation: 'always rotate the screenshot archive before publishing',
      workspacePath: '/ws',
    });
    insertMemory(db, {
      agentId: 'memory-agent',
      taskContext: 'deployment credentials',
      importance: 7,
      label: 'DECISION',
      observation: 'token expiry requires refreshing credentials before deploy',
      workspacePath: '/ws',
    });

    const relevant = notifyGet(db, {
      agent_id: 'agent-selective',
      session_id: 'session-relevant',
      workspace: '/ws',
      format: 'hook',
      query: 'fix token expiry during deployment',
    }) as { count: number; additionalContext?: string };
    expect(relevant.count).toBe(1);
    expect(relevant.additionalContext).toContain('token expiry');
    expect(relevant.additionalContext).not.toContain('screenshot archive');

    const unrelated = notifyGet(db, {
      agent_id: 'agent-selective',
      session_id: 'session-unrelated',
      workspace: '/ws',
      format: 'hook',
      query: 'format the release notes',
    });
    expect(unrelated).toEqual({ ok: true, count: 0, notifications: [] });
  });

  it('finds a grounded memory beyond higher-ranked one-token candidates', () => {
    const db = freshDb();
    for (const [index, token] of ['token', 'expiry', 'deployment', 'credentials'].entries()) {
      insertMemory(db, {
        agentId: 'memory-agent',
        taskContext: `dominant ${token}`,
        observation: `${token} unrelated singleton ${index}`,
        importance: 10,
        label: 'GOTCHA',
        workspacePath: '/ws',
      });
    }
    insertMemory(db, {
      agentId: 'memory-agent',
      taskContext: 'relevant token expiry',
      observation: 'token expiry fix requires refresh',
      importance: 6,
      label: 'DECISION',
      workspacePath: '/ws',
    });

    const result = notifyGet(db, {
      agent_id: 'agent-grounded',
      session_id: 'session-grounded',
      workspace: '/ws',
      format: 'hook',
      query: 'token expiry deployment credentials',
    }) as { count: number; additionalContext?: string };

    expect(result.count).toBe(1);
    expect(result.additionalContext).toContain('token expiry fix requires refresh');
    expect((result.additionalContext?.match(/Memory lead/g) ?? [])).toHaveLength(1);
  });

  it('does not persist the transient prompt in memory or delivery state', () => {
    const db = freshDb();
    insertMemory(db, {
      agentId: 'memory-agent',
      taskContext: 'private deployment token',
      observation: 'private deployment token requires rotation',
      importance: 8,
      label: 'SECURITY',
      workspacePath: '/ws',
    });
    const prompt = 'private deployment token rotate-now-secret';
    const before = (db.prepare('SELECT COUNT(*) AS count FROM memories').get() as { count: number }).count;

    const result = notifyGet(db, {
      agent_id: 'agent-private',
      session_id: 'session-private',
      workspace: '/ws',
      format: 'hook',
      query: prompt,
    }) as { additionalContext?: string };

    expect(result.additionalContext).toContain('private deployment token');
    expect((db.prepare('SELECT COUNT(*) AS count FROM memories').get() as { count: number }).count).toBe(before);
    const delivery = db.prepare('SELECT fingerprint, scope_key FROM delivery_state').get() as {
      fingerprint: string; scope_key: string;
    };
    expect(delivery.fingerprint).toMatch(/^[a-f0-9]{64}$/);
    expect(`${delivery.fingerprint} ${delivery.scope_key}`).not.toContain(prompt);
    expect(`${delivery.fingerprint} ${delivery.scope_key}`).not.toContain('rotate-now-secret');
  });

  it('keeps a five-item hook briefing within one KiB', () => {
    const db = freshDb();
    for (let index = 0; index < 5; index += 1) {
      insertNotification(db, {
        agentId: `sender-${index}`,
        toAgent: 'target',
        kind: 'request',
        subject: `subject-${index}-${'界'.repeat(200)}`,
        body: '🧠'.repeat(500),
        files: [
          `/very/${'長い/'.repeat(30)}file-${index}.ts`,
          `/second/${'path/'.repeat(30)}file-${index}.ts`,
          `/third/${'path/'.repeat(30)}file-${index}.ts`,
        ],
        workspacePath: '/ws',
        importance: 8,
      });
    }

    const result = notifyGet(db, {
      agent_id: 'target',
      session_id: 'session-bounded',
      workspace: '/ws',
      format: 'hook',
    }) as { count: number; additionalContext?: string };

    expect(result.count).toBe(5);
    expect(Buffer.byteLength(result.additionalContext ?? '', 'utf8')).toBeLessThanOrEqual(1024);
    expect(result.additionalContext).toContain('subject-0');
    expect(result.additionalContext).toContain('files=3');
  });

  it('surfaces weakness cluster when failure_signature is present', () => {
    const db = freshDb();
    const sig = 'mechanism:test-timeout|cause:slow-io';
    insertMem(db, { failureSig: sig, importance: 6, workspacePath: '/ws' });
    insertMem(db, { failureSig: sig, importance: 6, workspacePath: '/ws' });

    const res = notifyGet(db, { agent_id: 'agent-c', workspace: '/ws' }) as {
      ok: true; count: number; notifications: Array<{ kind: string; text: string }>;
    };
    expect(res.notifications.some(n => n.kind === 'weakness')).toBe(true);
  });

  it('silences unchanged hook briefs per agent, session, and scope without acknowledging signals', () => {
    const db = freshDb();
    insertMem(db, {
      importance: 8,
      label: 'GOTCHA',
      observation: 'brief only when the actionable set changes',
      workspacePath: '/ws',
    });
    db.prepare(`INSERT INTO signals (
      signal_id, workspace_path, from_agent, to_agent, kind, subject, files_json, refs_json,
      thread_id, importance, status, created_at
    ) VALUES ('signal-1', '/ws', 'agent-a', 'agent-b', 'request', 'review', '[]', '[]',
      'signal-1', 8, 'open', strftime('%Y-%m-%dT%H:%M:%fZ','now'))`).run();

    const params = { agent_id: 'agent-b', session_id: 'sess-1', workspace: '/ws', format: 'hook' };
    const first = notifyGet(db, params) as { count: number; additionalContext?: string };
    const second = notifyGet(db, params) as { count: number; additionalContext?: string };

    expect(first.additionalContext).toContain('review');
    expect(second).toEqual({ ok: true, count: 0, notifications: [] });
    expect((db.prepare('SELECT COUNT(*) AS c FROM signal_reads').get() as { c: number }).c).toBe(0);

    const otherSession = notifyGet(db, { ...params, session_id: 'sess-2' }) as { additionalContext?: string };
    expect(otherSession.additionalContext).toContain('review');
  });
});
