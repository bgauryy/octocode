import { describe, expect, it } from 'vitest';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { connectDb } from '../src/db.js';
import { createPiAwarenessBridge, wirePiAwarenessHooks } from '../src/pi-hooks.js';
import { insertNotification } from '../src/notifications.js';
import { insertMemory } from '../src/memory.js';
function tempDb() {
    const dir = mkdtempSync(join(tmpdir(), 'oc-pi-hooks-'));
    return { dir, dbPath: join(dir, 'awareness.sqlite3'), cleanup: () => rmSync(dir, { recursive: true, force: true }) };
}

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
      'session_start',
      'input',
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
      expect(second).toBeUndefined();
      db.close();
    } finally {
      if (previousAgentId === undefined) delete process.env.OCTOCODE_AGENT_ID;
      else process.env.OCTOCODE_AGENT_ID = previousAgentId;
      tmp.cleanup();
    }
  });

  it('keeps the latest Pi prompt transient and injects only its relevant memory lead', async () => {
    const tmp = tempDb();
    const previousAgentId = process.env.OCTOCODE_AGENT_ID;
    process.env.OCTOCODE_AGENT_ID = 'pi-prompt-agent';
    try {
      const db = connectDb(tmp.dbPath);
      insertMemory(db, {
        agentId: 'memory-agent',
        taskContext: 'release screenshots',
        observation: 'rotate the screenshot archive before publishing',
        importance: 10,
        label: 'GOTCHA',
        workspacePath: tmp.dir,
      });
      insertMemory(db, {
        agentId: 'memory-agent',
        taskContext: 'deployment credentials',
        observation: 'token expiry requires refreshing credentials before deploy',
        importance: 7,
        label: 'DECISION',
        workspacePath: tmp.dir,
      });
      const bridge = createPiAwarenessBridge({ getDb: () => db });
      await bridge.handleInput({ text: 'fix token expiry during deployment' }, { cwd: tmp.dir });
      const result = await bridge.handleBeforeAgentStart({}, { cwd: tmp.dir });

      expect(String(result?.message?.content)).toContain('token expiry');
      expect(String(result?.message?.content)).not.toContain('screenshot archive');

      const next = await bridge.handleBeforeAgentStart({}, { cwd: tmp.dir });
      expect(next).toBeUndefined();
      db.close();
    } finally {
      if (previousAgentId === undefined) delete process.env.OCTOCODE_AGENT_ID;
      else process.env.OCTOCODE_AGENT_ID = previousAgentId;
      tmp.cleanup();
    }
  });

  it('clears a queued Pi prompt when a newer input is empty', async () => {
    const tmp = tempDb();
    const previousAgentId = process.env.OCTOCODE_AGENT_ID;
    process.env.OCTOCODE_AGENT_ID = 'pi-empty-input-agent';
    try {
      const db = connectDb(tmp.dbPath);
      insertMemory(db, {
        agentId: 'memory-agent',
        taskContext: 'deployment token expiry',
        observation: 'deployment token expiry requires rotation',
        importance: 8,
        label: 'SECURITY',
        workspacePath: tmp.dir,
      });
      const bridge = createPiAwarenessBridge({ getDb: () => db });
      await bridge.handleInput({ text: 'deployment token expiry' }, { cwd: tmp.dir });
      await bridge.handleInput({ text: '   ' }, { cwd: tmp.dir });

      expect(await bridge.handleBeforeAgentStart({}, { cwd: tmp.dir })).toBeUndefined();
      db.close();
    } finally {
      if (previousAgentId === undefined) delete process.env.OCTOCODE_AGENT_ID;
      else process.env.OCTOCODE_AGENT_ID = previousAgentId;
      tmp.cleanup();
    }
  });

  it('isolates queued Pi prompts by session', async () => {
    const tmp = tempDb();
    const previousAgentId = process.env.OCTOCODE_AGENT_ID;
    process.env.OCTOCODE_AGENT_ID = 'pi-isolated-agent';
    try {
      const db = connectDb(tmp.dbPath);
      insertMemory(db, {
        agentId: 'memory-agent',
        taskContext: 'deployment token expiry',
        observation: 'deployment token expiry requires rotation',
        importance: 8,
        label: 'SECURITY',
        workspacePath: tmp.dir,
      });
      const bridge = createPiAwarenessBridge({ getDb: () => db });
      const sessionA = { cwd: tmp.dir, sessionManager: { getSessionFile: () => 'session-a.jsonl' } };
      const sessionB = { cwd: tmp.dir, sessionManager: { getSessionFile: () => 'session-b.jsonl' } };
      await bridge.handleInput({ text: 'deployment token expiry' }, sessionA);

      expect(await bridge.handleBeforeAgentStart({}, sessionB)).toBeUndefined();
      expect(String((await bridge.handleBeforeAgentStart({}, sessionA))?.message?.content))
        .toContain('deployment token expiry');
      db.close();
    } finally {
      if (previousAgentId === undefined) delete process.env.OCTOCODE_AGENT_ID;
      else process.env.OCTOCODE_AGENT_ID = previousAgentId;
      tmp.cleanup();
    }
  });

  it('sends a verify-gate follow-up message when pending runs remain', async () => {
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
      for (let index = 0; index < 5; index += 1) {
        await bridge.handleToolCall({ toolName: 'write', toolCallId: `tool-verify-${index}`, input: { path: `src/${index}.ts` } }, ctx);
        await bridge.handleToolResult({ toolCallId: `tool-verify-${index}` }, ctx);
      }

      await handlers.get('agent_end')?.({}, ctx);
      await handlers.get('agent_end')?.({}, ctx);

      expect(sent).toHaveLength(1);
      expect(sent[0]?.message.customType).toBe('octocode-awareness-verify-gate');
      expect(String(sent[0]?.message.content)).toContain('unverified edits');
      expect((String(sent[0]?.message.content).match(/PENDING:run_/g) ?? [])).toHaveLength(1);
      expect(String(sent[0]?.message.content)).not.toContain('omitted');
      expect(sent[0]?.options).toEqual({ deliverAs: 'followUp', triggerTurn: true });
      const aggregate = db.prepare("SELECT run_id, test_plan, status FROM task_runs WHERE origin = 'HOOK'").get() as {
        run_id: string;
        test_plan: string;
        status: string;
      };
      expect(aggregate.status).toBe('PENDING');
      expect(aggregate.test_plan).toContain('smallest relevant test/typecheck');
      expect(aggregate.test_plan).toContain('+2 more');
      expect((db.prepare('SELECT COUNT(*) AS c FROM run_files WHERE run_id = ?').get(aggregate.run_id) as { c: number }).c).toBe(5);
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
