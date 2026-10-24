import fs from 'node:fs';
import path from 'node:path';
import { describe, expect, it, vi } from 'vitest';
import { getAgentDir } from '@earendil-works/pi-coding-agent';
import { DEFAULT_BASH_TIMEOUT, MAX_FOREGROUND_TIMEOUT, effectiveTimeout, registerBashTool } from '../src/files/bash.js';
import { BashJobs, prefixed, shellSettings, type ShellSettings } from '../src/files/bash-jobs.js';
import { jobReport } from '../src/files/bash-report.js';
import { Subcommands } from '../src/shared/commands.js';
import { setCurrentSession } from '../src/shared/home.js';
import { groupAlive, processAlive } from '../src/shared/process.js';
import { fakeCtx, fakePi } from './fake-pi.js';
import { tmp, until } from './helpers.js';

describe('bash deadline', () => {
  it('honours the timeout a call asks for, longer or shorter, up to the foreground cap, and fills a missing one with the default', () => {
    expect(DEFAULT_BASH_TIMEOUT).toBeLessThan(MAX_FOREGROUND_TIMEOUT);
    const fallback = { seconds: DEFAULT_BASH_TIMEOUT, clamped: false };
    expect(effectiveTimeout(undefined)).toEqual(fallback);
    expect(effectiveTimeout(30)).toEqual({ seconds: 30, clamped: false });
    expect(effectiveTimeout(MAX_FOREGROUND_TIMEOUT)).toEqual({ seconds: MAX_FOREGROUND_TIMEOUT, clamped: false });
    expect(effectiveTimeout(0)).toEqual(fallback);
    expect(effectiveTimeout(-1)).toEqual(fallback);
    expect(effectiveTimeout(Number.NaN)).toEqual(fallback);
  });

  it('clamps a longer foreground timeout to the cap and says so', () => {
    expect(effectiveTimeout(MAX_FOREGROUND_TIMEOUT + 1)).toEqual({ seconds: MAX_FOREGROUND_TIMEOUT, clamped: true });
    expect(effectiveTimeout(1e6)).toEqual({ seconds: MAX_FOREGROUND_TIMEOUT, clamped: true });
    expect(effectiveTimeout(Number.POSITIVE_INFINITY)).toEqual({ seconds: DEFAULT_BASH_TIMEOUT, clamped: false });
  });
});

/** The note a clamped foreground call carries. */
const CLAMPED = `(timeout clamped to ${MAX_FOREGROUND_TIMEOUT} s; use background: true for longer)`;

describe('bash tool', () => {
  const setup = (shell: ShellSettings = {}) => {
    const { pi, tools, commands: registered, sent, fire } = fakePi();
    const commands = new Subcommands();
    const jobs = new BashJobs(tmp());
    registerBashTool(pi, commands, jobs, () => shell);
    const cwd = tmp();
    const ctx = fakeCtx({ cwd });
    return { tool: tools.get('bash'), registered, commands, jobs, sent, fire, cwd, ctx };
  };

  it('replaces Pi bash with a schema that has background and a documented deadline', () => {
    const { tool } = setup();
    expect(tool.name).toBe('bash');
    expect(Object.keys(tool.parameters.properties)).toEqual(['command', 'timeout', 'background']);
    expect(tool.promptGuidelines).toEqual([]);
    expect(tool.parameters.properties.timeout.description).toContain(`foreground default ${DEFAULT_BASH_TIMEOUT}s`);
    expect(tool.parameters.properties.background.description).toContain('long builds');
    // Pi's bash output, or a background start: codemode scripts get structuredContent only when this is declared.
    const variants = tool.outputSchema.anyOf.map((variant: { properties: object }) => Object.keys(variant.properties));
    expect(variants).toEqual([expect.arrayContaining(['output', 'exit_code']), ['job', 'pid', 'log']]);
  });

  it('runs a foreground command in the session cwd and returns its output', async () => {
    const { tool, ctx, cwd } = setup();
    const result = await tool.execute('t1', { command: 'pwd; echo hi' }, undefined, undefined, ctx);
    expect(result.content[0].text).toContain(fs.realpathSync(cwd));
    expect(result.content[0].text).toContain('hi');
  });

  it('does not start a background command after its request was aborted', async () => {
    const { tool, ctx, jobs, cwd } = setup();
    const controller = new AbortController();
    controller.abort();
    try {
      await expect(tool.execute('cancel-background', { command: 'echo started > cancelled.txt', background: true }, controller.signal, undefined, ctx)).rejects.toThrow(/abort/i);
      expect(fs.existsSync(path.join(cwd, 'cancelled.txt'))).toBe(false);
      expect(jobs.jobs.size).toBe(0);
    } finally {
      await jobs.stopAll('session end');
    }
  });

  it('stops a foreground command at its deadline and says how to run it longer', async () => {
    const { tool, ctx } = setup();
    const started = Date.now();
    await expect(tool.execute('t1', { command: 'echo before; sleep 30', timeout: 1 }, undefined, undefined, ctx)).rejects.toThrow(/before[\s\S]*Command timed out after 1 seconds[\s\S]*background: true/);
    expect(Date.now() - started).toBeLessThan(10_000);
  });

  it('carries the clamp note on a command that failed for another reason: a non-zero exit, and an abort', async () => {
    const { tool, ctx } = setup();
    const failed = await tool.execute('t1', { command: 'echo oops; exit 3', timeout: 1_000_000 }, undefined, undefined, ctx);
    expect(failed.content[0].text).toMatch(new RegExp(String.raw`oops[\s\S]*exited with code 3[\s\S]*${CLAMPED.replace(/[()]/g, '\\$&')}`));
    const controller = new AbortController();
    setTimeout(() => controller.abort(), 300);
    await expect(tool.execute('t2', { command: 'echo started; sleep 30', timeout: 1_000_000 }, controller.signal, undefined, ctx)).rejects.toThrow(new RegExp(String.raw`aborted[\s\S]*${CLAMPED.replace(/[()]/g, '\\$&')}`));
    const plain = new AbortController();
    setTimeout(() => plain.abort(), 300);
    const message = await tool.execute('t3', { command: 'sleep 30', timeout: 60 }, plain.signal, undefined, ctx).catch((error: Error) => error.message);
    expect(message).toMatch(/aborted/);
    expect(message).not.toMatch(/clamped/);
  });

  it('states in the result that a foreground timeout above an hour was clamped, and says nothing otherwise', async () => {
    const { tool, ctx } = setup();
    const clamped = await tool.execute('t1', { command: 'echo done', timeout: 1_000_000 }, undefined, undefined, ctx);
    expect(clamped.content[0].text).toContain('done');
    expect(clamped.content[0].text).toContain(CLAMPED);
    const plain = await tool.execute('t2', { command: 'echo done', timeout: MAX_FOREGROUND_TIMEOUT }, undefined, undefined, ctx);
    expect(plain.content[0].text).not.toMatch(/clamped/);
    expect(tool.parameters.properties.timeout.description).toContain(`at most ${MAX_FOREGROUND_TIMEOUT}s`);
    expect(tool.parameters.properties.timeout).not.toHaveProperty('maximum');
  });

  it('moves the full output of a truncated command out of the temp dir into its own folder', async () => {
    const { tool, ctx, jobs } = setup();
    const result = await tool.execute('t1', { command: 'seq 1 5000' }, undefined, undefined, ctx);
    const saved = result.details.fullOutputPath as string;
    expect(path.dirname(saved)).toBe(jobs.logDir());
    expect(fs.readFileSync(saved, 'utf8').split('\n')[4999]).toBe('5000');
    expect(result.content[0].text).toContain(`Full output: ${saved}]`);
    // A failing command (Pi returns it as an error result) points at the moved file too.
    const failed = await tool.execute('t2', { command: 'seq 1 5000; exit 1' }, undefined, undefined, ctx);
    expect(failed.isError).toBe(true);
    const moved = failed.details.fullOutputPath as string;
    expect(path.dirname(moved)).toBe(jobs.logDir());
    expect(fs.existsSync(moved)).toBe(true);
    expect(failed.content[0].text).toContain(`Full output: ${moved}]`);
  });

  it('moves the spill file of a command that timed out, though only its updates named it', async () => {
    const { tool, ctx, jobs } = setup();
    const message = await tool.execute('t1', { command: 'seq 1 5000; sleep 30', timeout: 1 }, undefined, undefined, ctx).catch((error: Error) => error.message);
    const moved = /Full output: (.+?)\]/.exec(message)![1]!;
    expect(path.dirname(moved)).toBe(jobs.logDir());
    expect(fs.readFileSync(moved, 'utf8')).toContain('5000');
  });

  it('runs foreground and background commands with the shellCommandPrefix and shellPath from Pi settings', async () => {
    const { tool, ctx, sent } = setup({ commandPrefix: 'export OCTO_PREFIX=set', shellPath: '/bin/sh' });
    const foreground = await tool.execute('t1', { command: 'echo "prefix=$OCTO_PREFIX"' }, undefined, undefined, ctx);
    expect(foreground.content[0].text).toContain('prefix=set');
    const result = await tool.execute('t2', { command: 'echo "prefix=$OCTO_PREFIX"', background: true }, undefined, undefined, ctx);
    await until(() => sent.length === 1);
    expect(sent[0]!.message.content).toContain('prefix=set');
    expect(result.content[0].text).toMatch(/Started bash-1/);
  });

  it('reads shell settings from Pi settings files and survives unreadable ones', () => {
    expect(shellSettings(tmp(), false)).toEqual(expect.any(Object));
    expect(prefixed('ls', 'set -e')).toBe('set -e\nls');
    expect(prefixed('ls')).toBe('ls');
  });

  it('keeps every rotated log within its cap when one chunk is larger than the cap', async () => {
    const jobs = new BashJobs(tmp());
    let exited = false;
    // One 40 KB burst of short lines, then one 20 KB line: both arrive in chunks larger than the 8 KB cap.
    const job = await jobs.start("awk 'BEGIN { for (i = 0; i < 2000; i++) printf \"line %05d padding\\n\", i; s = sprintf(\"%20000s\", \"\"); gsub(/ /, \"x\", s); print s; print \"end\" }'", tmp(), undefined, () => (exited = true), { maxLogBytes: 8 * 1024 });
    await until(() => exited);
    for (const file of [job.log, `${job.log}.1`]) expect(fs.statSync(file).size).toBeLessThanOrEqual(8 * 1024);
    expect(fs.readFileSync(job.log, 'utf8')).toMatch(/end\n$/);
  });

  it('rotates a background job log at its cap without losing or reordering output', async () => {
    const jobs = new BashJobs(tmp());
    let exited = false;
    let exit: Parameters<typeof jobReport>[1] | undefined;
    const job = await jobs.start('i=0; while [ $i -lt 3000 ]; do echo "line $i padding padding padding"; i=$((i+1)); done', tmp(), undefined, (_job, done) => ((exited = true), (exit = done)), { maxLogBytes: 8 * 1024 });
    await until(() => exited);
    // The report to the model says the log rotated, where the older part is, and that older output than that is gone.
    expect(exit!.logRotations).toBeGreaterThan(1);
    expect(jobReport(job, exit!)).toContain(`log rotated at its size cap ${exit!.logRotations} times: ${job.log}.1 holds the part before this log; output before that was dropped and cannot be recovered`);
    const log = fs.readFileSync(job.log, 'utf8');
    const previous = fs.readFileSync(`${job.log}.1`, 'utf8');
    expect(Buffer.byteLength(log)).toBeLessThanOrEqual(8 * 1024);
    expect(log).toMatch(new RegExp(`^\\[earlier output is in ${path.basename(job.log)}\\.1; older output was dropped\\]`));
    // Whatever both files keep is one unbroken run of lines ending at the last one.
    const numbers = [...`${previous}${log}`.matchAll(/^line (\d+) /gm)].map((match) => Number(match[1]));
    expect(numbers.at(-1)).toBe(2999);
    expect(numbers.every((n, i) => i === 0 || n === numbers[i - 1]! + 1)).toBe(true);
    expect(numbers.length).toBeGreaterThan(100);
  });

  it('keeps job logs in the current session folder under the Octocode home by default', () => {
    const home = tmp();
    const saved = process.env['OCTOCODE_HOME'];
    process.env['OCTOCODE_HOME'] = home;
    try {
      expect(new BashJobs().logDir()).toBe(path.join(home, 'agent', 'pi', 'sessions', `_pid-${process.pid}`, 'bash'));
      setCurrentSession('s:1');
      expect(new BashJobs().logDir()).toBe(path.join(home, 'agent', 'pi', 'sessions', 's_1', 'bash'));
    } finally {
      setCurrentSession(undefined);
      if (saved === undefined) delete process.env['OCTOCODE_HOME'];
      else process.env['OCTOCODE_HOME'] = saved;
    }
  });

  it('keeps the exit code of a failing foreground command', async () => {
    const { tool, ctx } = setup();
    // Pi 0.99 returns a non-zero exit as an error result (with structured output) rather than throwing.
    const result = await tool.execute('t1', { command: 'echo oops; exit 3' }, undefined, undefined, ctx);
    expect(result.isError).toBe(true);
    expect(result.content[0].text).toMatch(/oops[\s\S]*exited with code 3/);
    expect(result.structuredContent.exit_code).toBe(3);
  });

  it('starts a background job at once and reports its exit and output tail as a follow-up message', async () => {
    const { tool, ctx, jobs, sent } = setup();
    const result = await tool.execute('t1', { command: 'echo started; sleep 0.3; echo done; exit 2', background: true }, undefined, undefined, ctx);
    const text = result.content[0].text as string;
    expect(text).toMatch(/^Started bash-1 in the background \(pid \d+\)/);
    expect(text).toMatch(/Stop: kill -- -\d+/);
    expect(result.structuredContent).toEqual({ job: 'bash-1', pid: expect.any(Number), log: expect.stringMatching(/bash-1-\d+\.log$/) });
    expect(jobs.jobs.size).toBe(1);
    expect(ctx.ui.statuses.get('octocode-bash')).toBe('1 bash job');
    await until(() => sent.length === 1);
    expect(jobs.jobs.size).toBe(0);
    const { message, options } = sent[0]!;
    expect(options).toEqual({ triggerTurn: true, deliverAs: 'followUp' });
    expect(message.content).toMatch(/^Background bash bash-1 failed with exit code 2 after \d+s: echo started/);
    expect(message.content).toMatch(/started\ndone/);
  });

  it('caps a background timeout at the longest timer Node keeps instead of stopping the job at once', async () => {
    const { tool, ctx, jobs, sent } = setup();
    const result = await tool.execute('t1', { command: 'sleep 0.3', background: true, timeout: 3_000_000 }, undefined, undefined, ctx);
    expect(result.content[0].text).toMatch(/stopped after 596h31m/);
    expect(jobs.jobs.get('bash-1')?.timeout).toBe(2_147_483);
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(jobs.jobs.size).toBe(1);
    await until(() => sent.length === 1);
    expect(sent[0]!.message.content).not.toMatch(/stopped/);
  });

  it('gives a background job the environment Pi gives a foreground command, with a private log', async () => {
    const { tool, ctx, sent } = setup();
    vi.stubEnv('PI_MODEL', 'stale-model');
    vi.stubEnv('PI_SESSION_FILE', '/stale/session');
    vi.stubEnv('PATH', ['/usr/bin', '/bin'].join(path.delimiter));
    try {
      const command = 'printf "%s|%s|%s|%s" "$PI_SESSION_ID" "${PI_MODEL-unset}" "${PI_SESSION_FILE-unset}" "$PATH"';
      const foreground = (await tool.execute('t0', { command }, undefined, undefined, ctx)).content[0].text as string;
      const result = await tool.execute('t1', { command, background: true }, undefined, undefined, ctx);
      await until(() => sent.length === 1);
      const log = /Log: (\S+)/.exec(result.content[0].text as string)![1]!;
      const background = fs.readFileSync(log, 'utf8');
      expect(background).toBe(foreground.trim());
      expect(background).toBe(`session-test|unset|unset|${[path.join(getAgentDir(), 'bin'), '/usr/bin', '/bin'].join(path.delimiter)}`);
      if (process.platform !== 'win32') expect(fs.statSync(log).mode & 0o777).toBe(0o600);
    } finally {
      vi.unstubAllEnvs();
    }
  });

  it('stops a background job at its own timeout', async () => {
    const { tool, ctx, sent } = setup();
    await tool.execute('t1', { command: 'sleep 30', background: true, timeout: 1 }, undefined, undefined, ctx);
    await until(() => sent.length === 1, 8_000);
    expect(sent[0]!.message.content).toMatch(/stopped after its 1s timeout/);
  });

  it('lists and kills jobs with /octocode jobs, and reports the ones session end stops without waking anyone', async () => {
    const { tool, ctx, commands, jobs, sent, fire } = setup();
    await tool.execute('t1', { command: 'sleep 30', background: true }, undefined, undefined, ctx);
    await tool.execute('t2', { command: 'sleep 30', background: true }, undefined, undefined, ctx);
    const [first, second] = [...jobs.jobs.values()];
    await commands.get('jobs')!.handler('', ctx);
    expect(ctx.ui.notes.at(-1)!.message).toMatch(/bash-1 · \d+s · pid \d+ · sleep 30\n  log: .*bash-1-\d+\.log\nbash-2/);
    expect(commands.complete('jobs kill ')?.map((item) => item.value)).toEqual(['jobs kill bash-1', 'jobs kill bash-2']);
    await commands.get('jobs')!.handler('kill bash-1', ctx);
    await until(() => sent.length === 1);
    expect(sent[0]!.message.content).toMatch(/bash-1 stopped \(user\)/);
    expect(processAlive(first!.pid)).toBe(false);
    await commands.get('jobs')!.handler('kill nope', ctx);
    expect(ctx.ui.notes.at(-1)!.message).toMatch(/No background bash job "nope"/);
    expect(sent[0]!.message.details).toEqual({ id: 'bash-1', log: first!.log });
    // The user stopped it: the model learns of it without a turn being started for it.
    expect(sent[0]!.options).toEqual({ triggerTurn: false, deliverAs: 'followUp' });
    await fire('session_shutdown', {}, ctx);
    await second!.done;
    await new Promise((resolve) => setTimeout(resolve, 20));
    // Recorded once, before the stop, so a resumed session knows it did not finish; the exit adds nothing.
    expect(sent).toHaveLength(2);
    expect(sent[1]!.message).toMatchObject({ customType: 'octocode-bash-job', details: { id: 'bash-2', log: second!.log } });
    expect(sent[1]!.message.content).toMatch(/bash-2 stopped \(session end\)/);
    expect(sent[1]!.options).toEqual({ triggerTurn: false, deliverAs: 'followUp' });
  });

  it('starts a background job and stops it at session end', async () => {
    const { tool, jobs } = setup();
    const result = await tool.execute('t1', { command: 'sleep 30', background: true }, undefined, undefined, fakeCtx({ cwd: process.cwd() }));
    expect(result.content[0].text).toMatch(/background/);
    await jobs.stopAll('session end');
    await Promise.all([...jobs.jobs.values()].map((job) => job.done));
  });

  it('keeps a job running while children its shell left behind still run, so they stay stoppable', async () => {
    const jobs = new BashJobs(tmp());
    let exited = false;
    const job = await jobs.start('sleep 30 & echo started', tmp(), undefined, () => (exited = true));
    await new Promise((resolve) => setTimeout(resolve, 600));
    // The shell is gone, its `sleep 30` is not: the job is still listed and stop() reaches it.
    expect(exited).toBe(false);
    expect(jobs.jobs.has(job.id)).toBe(true);
    expect(jobs.stop(job.id, 'user')).toBe(true);
    await job.done;
    expect(groupAlive(job.pid)).toBe(false);
  });

  it('escalates to SIGKILL before stopAll resolves, for a job that ignores SIGTERM', async () => {
    const jobs = new BashJobs(tmp());
    const job = await jobs.start("trap '' TERM; echo ready; while :; do sleep 0.05; done", tmp(), undefined, () => undefined);
    await until(() => fs.readFileSync(job.log, 'utf8').includes('ready'));
    await jobs.stopAll('session end');
    expect(groupAlive(job.pid)).toBe(false);
  });

  it('settles its jobs for a headless run: waits for timed ones, stops untimed ones, reports both', async () => {
    const { tool, jobs, sent, cwd } = setup();
    const headless = fakeCtx({ cwd, hasUI: false });
    await tool.execute('t1', { command: 'sleep 0.3; echo timed-done', background: true, timeout: 20 }, undefined, undefined, headless);
    await tool.execute('t2', { command: 'sleep 60', background: true }, undefined, undefined, headless);
    // settle() is what index.ts holds a headless run on: it stops the untimed job and waits for the timed one.
    await jobs.settle();
    await vi.waitFor(() => expect(sent).toHaveLength(2), { timeout: 5_000 });
    const reports = sent.map((entry) => String(entry.message.content)).sort();
    expect(reports.some((text) => /finished after .*sleep 0\.3; echo timed-done/.test(text) && text.includes('timed-done'))).toBe(true);
    expect(reports.some((text) => /stopped \(run end\)/.test(text))).toBe(true);
    expect(jobs.jobs.size).toBe(0);
  });

  it('starts more than eight jobs at once: there is no running limit', async () => {
    const jobs = new BashJobs(tmp());
    const cwd = tmp();
    for (let index = 0; index < 9; index++) await jobs.start('sleep 30', cwd, undefined, () => undefined);
    expect(jobs.jobs.size).toBe(9);
    await jobs.stopAll('session end');
    await Promise.all([...jobs.jobs.values()].map((job) => job.done));
  });
});
