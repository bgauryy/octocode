import { describe, expect, it } from 'vitest';
import { mkdirSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { connectDb } from '../src/db.js';
import { createPiAwarenessBridge, extractPiWriteTargetPaths, wirePiAwarenessHooks } from '../src/pi-hooks.js';
import { preFlightIntent } from '../src/intents.js';
import { insertNotification } from '../src/notifications.js';

function tempDb() {
  const dir = mkdtempSync(join(tmpdir(), 'oc-pi-hooks-'));
  return { dir, dbPath: join(dir, 'awareness.sqlite3'), cleanup: () => rmSync(dir, { recursive: true, force: true }) };
}

describe('extractPiWriteTargetPaths', () => {
  it('extracts Pi write/edit tool input shapes', () => {
    expect(extractPiWriteTargetPaths('write', { path: 'src/a.ts' })).toEqual(['src/a.ts']);
    expect(extractPiWriteTargetPaths('edit', { file_path: 'src/b.ts', filePaths: ['src/c.ts', 'src/b.ts'] })).toEqual([
      'src/b.ts',
      'src/c.ts',
    ]);
    expect(extractPiWriteTargetPaths('edit', {
      queries: [
        { path: 'src/d.ts' },
        { file_path: 'src/e.ts', filePaths: ['src/f.ts', 'src/d.ts'] },
      ],
    })).toEqual(['src/d.ts', 'src/e.ts', 'src/f.ts']);
  });

  it('extracts apply_patch file paths from command payloads', () => {
    expect(extractPiWriteTargetPaths('bash', {
      command: ['*** Begin Patch', '*** Update File: src/a.ts', '*** Move to: src/b.ts', '*** End Patch'].join('\n'),
    })).toEqual(['src/a.ts', 'src/b.ts']);
  });
});

describe('createPiAwarenessBridge', () => {
  it('claims and releases Pi tool writes through the shared DB', async () => {
    const tmp = tempDb();
    try {
      const db = connectDb(tmp.dbPath);
      const bridge = createPiAwarenessBridge({ getDb: () => db });
      const ctx = { cwd: tmp.dir, sessionManager: { getSessionFile: () => join(tmp.dir, 'session.jsonl') } };

      await bridge.handleToolCall({ toolName: 'write', toolCallId: 'tool-1', input: { path: 'src/a.ts' } }, ctx);
      expect(bridge.pendingToolFiles.get('tool-1')).toEqual(['src/a.ts']);
      expect(bridge.pendingToolTasks.get('tool-1')).toMatch(/^task_/);
      expect((db.prepare("SELECT COUNT(*) AS c FROM tasks WHERE status='ACTIVE'").get() as { c: number }).c).toBe(1);

      await bridge.handleToolResult({ toolCallId: 'tool-1' }, ctx);
      expect(bridge.pendingToolFiles.has('tool-1')).toBe(false);
      expect((db.prepare("SELECT COUNT(*) AS c FROM tasks WHERE status='PENDING'").get() as { c: number }).c).toBe(1);
      expect((db.prepare('SELECT COUNT(*) AS c FROM locks').get() as { c: number }).c).toBe(0);
      expect((db.prepare('SELECT COUNT(*) AS c FROM edit_log').get() as { c: number }).c).toBe(1);
      db.close();
    } finally {
      tmp.cleanup();
    }
  });

  it('blocks Pi writes when another agent holds the file', async () => {
    const tmp = tempDb();
    try {
      const db = connectDb(tmp.dbPath);
      preFlightIntent(db, { agentId: 'other', targetFiles: ['src/conflict.ts'], workspacePath: tmp.dir });
      const bridge = createPiAwarenessBridge({ getDb: () => db });

      const result = await bridge.handleToolCall(
        { toolName: 'edit', toolCallId: 'tool-2', input: { path: 'src/conflict.ts' } },
        { cwd: tmp.dir, sessionManager: { getSessionFile: () => join(tmp.dir, 'mine.jsonl') } },
      );

      expect(result).toMatchObject({ block: true });
      expect(result?.reason).toContain('other');
      db.close();
    } finally {
      tmp.cleanup();
    }
  });

  it('releases only the matching task for overlapping same-agent tool calls', async () => {
    const tmp = tempDb();
    try {
      const db = connectDb(tmp.dbPath);
      const bridge = createPiAwarenessBridge({ getDb: () => db });
      const ctx = { cwd: tmp.dir, sessionManager: { getSessionFile: () => join(tmp.dir, 'session.jsonl') } };

      await bridge.handleToolCall({ toolName: 'write', toolCallId: 'tool-1', input: { path: 'src/a.ts' } }, ctx);
      await bridge.handleToolCall({ toolName: 'write', toolCallId: 'tool-2', input: { path: 'src/a.ts' } }, ctx);
      const secondTask = bridge.pendingToolTasks.get('tool-2');
      expect((db.prepare('SELECT COUNT(*) AS c FROM locks').get() as { c: number }).c).toBe(2);

      await bridge.handleToolResult({ toolCallId: 'tool-1' }, ctx);
      expect((db.prepare('SELECT COUNT(*) AS c FROM locks').get() as { c: number }).c).toBe(1);
      const remaining = db.prepare('SELECT task_id FROM locks').get() as { task_id: string };
      expect(remaining.task_id).toBe(secondTask);

      const blocked = await createPiAwarenessBridge({ getDb: () => db }).handleToolCall(
        { toolName: 'edit', toolCallId: 'tool-3', input: { path: 'src/a.ts' } },
        { cwd: tmp.dir, sessionManager: { getSessionFile: () => join(tmp.dir, 'other.jsonl') } },
      );
      expect(blocked).toMatchObject({ block: true });
      db.close();
    } finally {
      tmp.cleanup();
    }
  });

  it('guards Pi harness self-edits with the same approval env as shell hooks', async () => {
    const tmp = tempDb();
    const previousAllow = process.env.OCTOCODE_ALLOW_HARNESS_APPLY;
    const previousBranchOk = process.env.OCTOCODE_HARNESS_BRANCH_OK;
    try {
      delete process.env.OCTOCODE_ALLOW_HARNESS_APPLY;
      delete process.env.OCTOCODE_HARNESS_BRANCH_OK;
      const skillRoot = join(tmp.dir, 'skills', 'octocode-awareness');
      mkdirSync(skillRoot, { recursive: true });
      const db = connectDb(tmp.dbPath);
      const bridge = createPiAwarenessBridge({ getDb: () => db, skillRoot });
      const ctx = { cwd: tmp.dir, sessionManager: { getSessionFile: () => join(tmp.dir, 'session.jsonl') } };

      const blocked = await bridge.handleToolCall(
        { toolName: 'write', toolCallId: 'guard-1', input: { path: join(skillRoot, 'SKILL.md') } },
        ctx,
      );
      expect(blocked).toMatchObject({ block: true });
      expect(blocked?.reason).toContain('editing the skill itself is gated');
      expect((db.prepare('SELECT COUNT(*) AS c FROM tasks').get() as { c: number }).c).toBe(0);

      process.env.OCTOCODE_ALLOW_HARNESS_APPLY = '1';
      process.env.OCTOCODE_HARNESS_BRANCH_OK = '1';

      const allowed = await bridge.handleToolCall(
        { toolName: 'write', toolCallId: 'guard-2', input: { path: join(skillRoot, 'README.md') } },
        ctx,
      );
      expect(allowed).toBeUndefined();
      expect((db.prepare('SELECT COUNT(*) AS c FROM tasks').get() as { c: number }).c).toBe(1);
      db.close();
    } finally {
      if (previousAllow === undefined) delete process.env.OCTOCODE_ALLOW_HARNESS_APPLY;
      else process.env.OCTOCODE_ALLOW_HARNESS_APPLY = previousAllow;
      if (previousBranchOk === undefined) delete process.env.OCTOCODE_HARNESS_BRANCH_OK;
      else process.env.OCTOCODE_HARNESS_BRANCH_OK = previousBranchOk;
      tmp.cleanup();
    }
  });
});

describe('wirePiAwarenessHooks', () => {
  it('registers Pi lifecycle equivalents for awareness hooks', () => {
    const events: string[] = [];
    const pi = { on: (eventName: string) => { events.push(eventName); } };

    const bridge = wirePiAwarenessHooks(pi);

    expect(bridge).toBeTruthy();
    expect(events).toEqual([
      'tool_call',
      'tool_result',
      'tool_execution_start',
      'tool_execution_end',
      'before_agent_start',
      'agent_end',
      'session_before_compact',
      'session_shutdown',
    ]);
  });


  it('delivers unread notifications through before_agent_start context', async () => {
    const tmp = tempDb();
    const previousAgentId = process.env.OCTOCODE_AGENT_ID;
    process.env.OCTOCODE_AGENT_ID = 'agent-b';
    try {
      const db = connectDb(tmp.dbPath);
      insertNotification(db, {
        agentId: 'agent-a',
        toAgent: 'agent-b',
        kind: 'handoff',
        subject: 'hook handoff works',
        body: 'check the notification path',
        workspacePath: tmp.dir,
      });
      const bridge = wirePiAwarenessHooks({ on: () => undefined }, { getDb: () => db })!;
      const result = await bridge.handleBeforeAgentStart({}, { cwd: tmp.dir });

      expect(result?.message?.customType).toBe('octocode-awareness-briefing');
      expect(String(result?.message?.content)).toContain('hook handoff works');
      expect(String(result?.message?.content)).toContain('agent-a');

      const second = await bridge.handleBeforeAgentStart({}, { cwd: tmp.dir });
      expect(String(second?.message?.content ?? '')).toContain('hook handoff works');
      db.close();
    } finally {
      if (previousAgentId === undefined) delete process.env.OCTOCODE_AGENT_ID;
      else process.env.OCTOCODE_AGENT_ID = previousAgentId;
      tmp.cleanup();
    }
  });

  it('sends a verify-gate follow-up message when pending tasks remain', async () => {
    const tmp = tempDb();
    try {
      const db = connectDb(tmp.dbPath);
      const handlers = new Map<string, (event: Record<string, unknown>, ctx: Record<string, unknown>) => Promise<unknown> | unknown>();
      const sent: Array<{ message: Record<string, unknown>; options?: Record<string, unknown> }> = [];
      const pi = {
        on: (eventName: string, handler: (event: Record<string, unknown>, ctx: Record<string, unknown>) => Promise<unknown> | unknown) => {
          handlers.set(eventName, handler);
        },
        sendMessage: (message: Record<string, unknown>, options?: Record<string, unknown>) => {
          sent.push({ message, options });
        },
      };
      wirePiAwarenessHooks(pi, { getDb: () => db });

      const ctx = { cwd: tmp.dir, sessionManager: { getSessionFile: () => join(tmp.dir, 'session.jsonl') } };
      const bridge = createPiAwarenessBridge({ getDb: () => db });
      await bridge.handleToolCall({ toolName: 'write', toolCallId: 'tool-verify', input: { path: 'src/a.ts' } }, ctx);
      await bridge.handleToolResult({ toolCallId: 'tool-verify' }, ctx);

      await handlers.get('agent_end')?.({}, ctx);

      expect(sent).toHaveLength(1);
      expect(sent[0]?.message.customType).toBe('octocode-awareness-verify-gate');
      expect(String(sent[0]?.message.content)).toContain('unverified edits');
      expect(sent[0]?.options).toEqual({ deliverAs: 'followUp', triggerTurn: true });
      db.close();
    } finally {
      tmp.cleanup();
    }
  });

  it('runs session_shutdown through the Pi hook without sending verify messages', async () => {
    const tmp = tempDb();
    try {
      const db = connectDb(tmp.dbPath);
      const handlers = new Map<string, (event: Record<string, unknown>, ctx: Record<string, unknown>) => Promise<unknown> | unknown>();
      const sent: unknown[] = [];
      const pi = {
        on: (eventName: string, handler: (event: Record<string, unknown>, ctx: Record<string, unknown>) => Promise<unknown> | unknown) => {
          handlers.set(eventName, handler);
        },
        sendMessage: (message: Record<string, unknown>) => sent.push(message),
      };
      wirePiAwarenessHooks(pi, { getDb: () => db });

      await expect(handlers.get('session_shutdown')?.({ reason: 'quit' }, { cwd: tmp.dir })).resolves.toBeUndefined();
      expect(sent).toHaveLength(0);
      db.close();
    } finally {
      tmp.cleanup();
    }
  });
});
