import fs from 'node:fs';
import path from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import octocode from '../src/index.js';
import type { ChildProgress } from '../src/subagents/events.js';
import { ABORT_SETTLE_MS, runSubagent, USER_STOP } from '../src/subagents/process.js';
import { fakeCtx, fakePi } from './fake-pi.js';
import { tmp } from './helpers.js';
import { useScriptedPi } from './subagent-tool-harness.js';


useScriptedPi();

afterEach(() => {
  delete process.env['OCTOCODE_FAKE_LOG'];
});

/** What the stand-in child recorded: `abort` when it got that command, `stdin-closed` when its input ended. */
function childLog(): { file: string; lines: () => string[] } {
  const file = path.join(tmp(), 'child.log');
  process.env['OCTOCODE_FAKE_LOG'] = file;
  return { file, lines: () => (fs.existsSync(file) ? fs.readFileSync(file, 'utf8').trim().split('\n') : []) };
}

describe('a subagent child over RPC', () => {
  it('runs a task sent as a prompt command, however long, with no byte of it in the arguments', async () => {
    const task = `echo task ${'x'.repeat(200_000)} end of task`;
    const outcome = await runSubagent(task, [], tmp(), undefined, { id: 'a-long' }, undefined, () => undefined);
    expect(outcome.text).toBe(`TASK ${task.length} ${JSON.stringify(task.slice(0, 40))} ${task.slice(-12)}`);
  });

  it('closes the child\'s stdin once its run settles, so it leaves by itself', async () => {
    const log = childLog();
    const outcome = await runSubagent('grants', [], tmp(), undefined, { id: 'a-settled' }, undefined, () => undefined);
    expect(outcome.text).toContain('mode=');
    expect(log.lines()).toEqual(['stdin-closed']);
  });

  it('ignores response and fire-and-forget records, reporting what JSON mode would', async () => {
    const seen: ChildProgress[] = [];
    const outcome = await runSubagent('dialogs', [], tmp(), undefined, { id: 'a-ui' }, undefined, (update) => seen.push(update));
    expect(outcome.text.startsWith('ANSWERS ')).toBe(true);
    // The notify, the setStatus and the prompt response are not activity: only the child's reply shows.
    expect(seen.filter((update) => update.activity)).toEqual([]);
    expect(seen.filter((update) => update.reply)).toHaveLength(1);
  });

  it('answers every dialog the child opens as cancelled, and nothing else', async () => {
    const outcome = await runSubagent('dialogs', [], tmp(), undefined, { id: 'a-dialogs' }, undefined, () => undefined);
    const [, answers, sent] = /^ANSWERS (.*) COMMANDS (.*)$/.exec(outcome.text)!;
    expect(JSON.parse(answers!)).toEqual(['d1', 'd2', 'd3', 'd4'].map((id) => ({ type: 'extension_ui_response', id, cancelled: true })));
    // The only commands the child received after its prompt are those four answers: none for notify or setStatus.
    expect(JSON.parse(sent!)).toEqual(['d1', 'd2', 'd3', 'd4']);
  });

  it('stops with abort, then agent_settled, then a closed stdin, without signalling a child that follows', async () => {
    const log = childLog();
    const controller = new AbortController();
    const run = runSubagent('abortable', [], tmp(), undefined, { id: 'a-abort' }, controller.signal, () => undefined);
    // The child reports `agent_start` first; abort once it is running.
    await new Promise((resolve) => setTimeout(resolve, 600));
    const started = Date.now();
    controller.abort();
    await expect(run).rejects.toThrow('Subagent cancelled');
    expect(log.lines()).toEqual(['abort', 'stdin-closed']);
    expect(Date.now() - started).toBeLessThan(ABORT_SETTLE_MS);
  }, 15_000);

  it('falls back to signals after the bounded wait when a child never settles', async () => {
    const log = childLog();
    const controller = new AbortController();
    const run = runSubagent('stubborn', [], tmp(), undefined, { id: 'a-unresponsive' }, controller.signal, () => undefined);
    await new Promise((resolve) => setTimeout(resolve, 600));
    controller.abort(USER_STOP);
    await expect(run).rejects.toThrow('Subagent cancelled');
    // It ignored the abort, the closed input and SIGTERM: only the SIGKILL ended it, and the input was closed first.
    expect(log.lines()).toEqual(['stdin-closed']);
  }, 20_000);

  it('fails a task the child rejects instead of waiting for a run that never starts', async () => {
    await expect(runSubagent('rejected', [], tmp(), undefined, { id: 'a-rejected' }, undefined, () => undefined)).rejects.toThrow('Subagent rejected its task: No model selected');
  }, 20_000);

  it('closes the child\'s input when an extension command consumed the prompt (no run, no agent_settled)', async () => {
    const log = childLog();
    const outcome = await runSubagent('handled', [], tmp(), undefined, { id: 'a-handled' }, undefined, () => undefined);
    expect(outcome.text).toBe('');
    expect(log.lines()).toEqual(['stdin-closed']);
  });
});

describe('the run-hold for background work in an RPC child', () => {
  const held = async (env: Record<string, string | undefined>, hasUI: boolean): Promise<boolean> => {
    const saved = { ...process.env };
    Object.assign(process.env, { OCTOCODE_MCP: '0', OCTOCODE_HOME: tmp(), ...env });
    const fake = fakePi();
    try {
      octocode(fake.pi);
      const ctx = fakeCtx({ cwd: tmp(), hasUI, mode: hasUI ? 'rpc' : 'json' });
      const controller = new AbortController();
      Object.assign(ctx, { signal: controller.signal });
      await fake.tools.get('bash').execute('t1', { command: 'sleep 30', background: true, timeout: 30 }, undefined, undefined, ctx);
      try {
        let settled = false;
        const pending = fake.fire('agent_before_settle', { type: 'agent_before_settle', outcome: 'completed', entries: [], continue: false }, ctx).then(() => (settled = true));
        await new Promise((resolve) => setTimeout(resolve, 300));
        const holding = !settled;
        controller.abort();
        await pending;
        return holding;
      } finally {
        await fake.emit('session_shutdown', { type: 'session_shutdown' }, ctx);
      }
    } finally {
      process.env = saved;
    }
  };

  it('still waits for a bash job when hasUI is true (RPC), because the session is a subagent', async () => {
    expect(await held({ OCTOCODE_SUBAGENT: '1' }, true)).toBe(true);
  }, 20_000);

  it('does not wait in a main session with a UI, and still waits in a headless one', async () => {
    expect(await held({ OCTOCODE_SUBAGENT: undefined }, true)).toBe(false);
    expect(await held({ OCTOCODE_SUBAGENT: undefined }, false)).toBe(true);
  }, 20_000);
});
