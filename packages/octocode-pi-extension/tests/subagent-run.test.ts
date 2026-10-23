import fs from 'node:fs';
import path from 'node:path';
import { describe, expect, it, vi } from 'vitest';
import { runSubagent } from '../src/subagents/process.js';
import { processAlive } from '../src/shared/process.js';
import { tmp } from './helpers.js';
import { reportOf, useScriptedPi } from './subagent-tool-harness.js';

useScriptedPi();

/** Runs `run` with `body` as the stand-in `pi` script, then kills the holder process the script recorded. */
const withScript = async (body: string, run: (holderFile: string) => Promise<void>) => {
  const dir = tmp();
  const holderFile = path.join(dir, 'holder.pid');
  const script = path.join(dir, 'pi');
  fs.writeFileSync(script, `const { spawn } = require('node:child_process');\n${body}`);
  const before = process.argv[1];
  process.argv[1] = script;
  process.env['TEST_HOLDER_FILE'] = holderFile;
  try {
    await run(holderFile);
  } finally {
    process.argv[1] = before!;
    delete process.env['TEST_HOLDER_FILE'];
    try { process.kill(Number(fs.readFileSync(holderFile, 'utf8')), 'SIGKILL'); } catch { /* Gone, or never started. */ }
  }
};

describe('runSubagent', () => {
  it.skipIf(process.platform === 'win32')('settles a cancelled run only once its tree is gone, including a detached helper the child left running', async () => {
    const controller = new AbortController();
    let helper = 0;
    const run = runSubagent('detached helper', [], tmp(), undefined, { id: 'a-helper' }, controller.signal, ({ activity }) => {
      const pid = Number(/"helper":(\d+)/.exec(activity ?? '')?.[1] ?? /helper[^\d]*(\d+)/.exec(activity ?? '')?.[1]);
      if (pid) helper = pid;
    });
    await vi.waitFor(() => expect(helper).toBeGreaterThan(0), { timeout: 10_000 });
    try {
      controller.abort();
      await expect(run).rejects.toThrow('Subagent cancelled');
      expect(processAlive(helper)).toBe(false);
    } finally {
      try { process.kill(helper, 'SIGKILL'); } catch { /* Already exited. */ }
    }
  }, 20_000);

  describe.skipIf(process.platform === 'win32')('after a natural exit', () => {
    // A stand-in `pi` that answers, leaves a holder in its own process group (like an MCP or dev server) and exits.
    const holder = `
      const holder = spawn(process.execPath, ['-e', 'setInterval(() => undefined, 1000)'], { stdio: 'ignore' });
      holder.unref();
      require('node:fs').writeFileSync(process.env.TEST_HOLDER_FILE, String(holder.pid));
      const usage = { input: 1, output: 1, cacheRead: 0, cacheWrite: 0, totalTokens: 2, cost: { total: 0 } };
      process.stdout.write(JSON.stringify({ type: 'message_end', message: { role: 'assistant', content: [{ type: 'text', text: 'DONE' }], usage, stopReason: 'stop' } }) + '\\n');
    `;

    it('stops what the child left in its process group before the run settles', async () => {
      await withScript(holder, async (holderFile) => {
        const outcome = await runSubagent('go', [], tmp(), undefined, { id: 'a-holder' }, undefined, () => undefined);
        expect(outcome.text).toBe('DONE');
        // The run has settled: the holder it left behind is already gone, so a worktree removal now races nothing.
        expect(processAlive(Number(fs.readFileSync(holderFile, 'utf8')))).toBe(false);
      });
    }, 20_000);
  });

  it('terminates the child when recording its launch fails', async () => {
    let pid = 0;
    await expect(runSubagent('slow launch', [], tmp(), undefined, { id: 'a-launch-failure' }, undefined, () => undefined, (childPid) => {
      pid = childPid;
      throw new Error('cannot record launch');
    })).rejects.toThrow('cannot record launch');
    try {
      expect(() => process.kill(pid, 0)).toThrow();
    } finally {
      try { process.kill(pid, 'SIGKILL'); } catch { /* Already exited. */ }
    }
  });

  it('rejects oversized unterminated events instead of buffering them without limit', async () => {
    await expect(runSubagent('oversized', [], tmp(), undefined, { id: 'a-oversized' }, undefined, () => undefined)).rejects.toThrow(/JSON event exceeds/);
  });
});

describe('runSubagent', () => {
  it('bounds the returned report while preserving the complete text in scratch', async () => {
    const scratch = tmp();
    const outcome = await runSubagent('long report', [], tmp(), undefined, { id: 'a-bounded', scratch }, undefined, () => undefined);
    expect(Buffer.byteLength(outcome.text)).toBeLessThan(9 * 1024);
    expect(outcome.text).toContain('Full report:');
    expect(fs.readFileSync(path.join(scratch, 'report.md'), 'utf8')).toMatch(/the end$/);
  });

  it('keeps the pointer to the saved partial report in a crash error, however long the narration was', async () => {
    const scratch = tmp();
    const error = await runSubagent('long narrate then crash', [], tmp(), undefined, { id: 'a-crash', scratch }, undefined, () => undefined).then(() => undefined, (e: Error) => e);
    expect(error?.message).toMatch(/^Subagent exited with code 3: segfault\nLast progress: progress detail .*\n\[Output trimmed: [^\]]+\]\nFull partial report: .*partial-report\.md/s);
    expect(fs.readFileSync(path.join(scratch, 'partial-report.md'), 'utf8')).toMatch(/the end$/);
  });

  it('streams the gist of its thinking and keeps thoughts and narration as steps', async () => {
    const seen: Array<{ step?: string; thinking?: string; activity?: string }> = [];
    const outcome = await runSubagent('think aloud', [], tmp(), undefined, { id: 'a-think' }, undefined, ({ step, thinking, activity }) => seen.push({ ...(step ? { step } : {}), ...(thinking !== undefined ? { thinking } : {}), ...(activity ? { activity } : {}) }));
    expect(outcome.text).toBe('THE ANSWER');
    expect(seen.filter((update) => Object.keys(update).length > 0)).toEqual([
      { thinking: 'Locating the bug' },
      { thinking: 'The token refresh runs twice.' },
      { step: '✻ Check the lock' },
      { step: '› Let me read src/auth.ts' },
      { thinking: '' },
      { activity: '→ read src/auth.ts' },
      { thinking: '' },
    ]);
  });

  it('rejects at once when already aborted', async () => {
    const controller = new AbortController();
    controller.abort();
    await expect(runSubagent('x', [], tmp(), undefined, { id: 'a-0000' }, controller.signal, () => undefined)).rejects.toThrow('Subagent cancelled');
  });

  it('reports the child pid and parses a final line without a newline', async () => {
    const pids: number[] = [];
    const outcome = await runSubagent('report', [], tmp(), undefined, { id: 'a-0001', goal: 't', workspace: '/w' }, undefined, () => undefined, (pid) => pids.push(pid));
    expect(pids).toHaveLength(1);
    expect(reportOf(outcome.text).id).toBe('a-0001');
    expect(outcome.usage.input).toBe(100);
  });

  it('keeps the report when a late notice wakes the child after it answered', async () => {
    const outcome = await runSubagent('late notice', [], tmp(), undefined, { id: 'a-0003' }, undefined, () => undefined);
    expect(outcome.text).toBe('FULL REPORT\n\n---\n\nThat job was my coverage rerun; findings unchanged.');
    expect(outcome.error).toBeUndefined();
  });

  it('reports a run done when only a follow-up turn after its answer failed', async () => {
    const outcome = await runSubagent('late failure', [], tmp(), undefined, { id: 'a-0004' }, undefined, () => undefined);
    expect(outcome.text).toBe('FULL REPORT\n\n---\n\n(A later follow-up turn failed: overloaded)');
    expect(outcome.error).toBeUndefined();
  });

  it('never keeps a failed draft as a report section', async () => {
    const outcome = await runSubagent('steered draft', [], tmp(), undefined, { id: 'a-0005' }, undefined, () => undefined);
    expect(outcome.text).toBe('FINAL REPORT');
    expect(outcome.error).toBeUndefined();
  });

  it.skipIf(process.platform === 'win32')('settles shortly after the child exits even when an escaped grandchild holds its pipes open', async () => {
    const started = Date.now();
    const outcome = await runSubagent('orphan holder', [], tmp(), undefined, { id: 'a-orphan' }, undefined, () => undefined);
    expect(outcome.text).toBe('ORPHAN REPORT');
    expect(Date.now() - started).toBeLessThan(5_000);
  }, 15_000);

  it('keeps running when the progress callback throws', async () => {
    const outcome = await runSubagent('report', [], tmp(), undefined, { id: 'a-0002' }, undefined, () => {
      throw new Error('render gone');
    });
    expect(reportOf(outcome.text).id).toBe('a-0002');
  });
});
