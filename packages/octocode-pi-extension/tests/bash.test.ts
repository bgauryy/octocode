import fs from 'node:fs';
import path from 'node:path';
import { describe, expect, it, vi } from 'vitest';
import { getAgentDir } from '@earendil-works/pi-coding-agent';
import { DEFAULT_BASH_TIMEOUT, bashTimeoutLimit, effectiveTimeout, registerBashTool } from '../src/files/bash.js';
import { BashJobs, capLog, jobReport, logTail, prefixed, shellSettings, type ShellSettings } from '../src/files/bash-jobs.js';
import { Subcommands } from '../src/shared/commands.js';
import { setCurrentSession } from '../src/shared/home.js';
import { processAlive } from '../src/shared/process.js';
import { fakeCtx, fakePi, rendered, theme } from './fake-pi.js';
import { tmp } from './helpers.js';

const until = async (check: () => boolean, ms = 5_000) => {
  const end = Date.now() + ms;
  while (!check()) {
    if (Date.now() > end) throw new Error('condition not met in time');
    await new Promise((resolve) => setTimeout(resolve, 20));
  }
};

describe('bash deadline', () => {
  it('defaults to 15 minutes, reads OCTOCODE_BASH_TIMEOUT, and ignores nonsense', () => {
    expect(DEFAULT_BASH_TIMEOUT).toBe(900);
    expect(bashTimeoutLimit({})).toBe(900);
    expect(bashTimeoutLimit({ OCTOCODE_BASH_TIMEOUT: '60' })).toBe(60);
    expect(bashTimeoutLimit({ OCTOCODE_BASH_TIMEOUT: '0' })).toBe(900);
    expect(bashTimeoutLimit({ OCTOCODE_BASH_TIMEOUT: '99999999' })).toBe(2_147_483);
    expect(bashTimeoutLimit({ OCTOCODE_BASH_TIMEOUT: 'soon' })).toBe(900);
  });

  it('fills a missing timeout with the limit and only lets a call shorten it', () => {
    expect(effectiveTimeout(undefined, 900)).toBe(900);
    expect(effectiveTimeout(30, 900)).toBe(30);
    expect(effectiveTimeout(5_000, 900)).toBe(900);
    expect(effectiveTimeout(0, 900)).toBe(900);
    expect(effectiveTimeout(-1, 900)).toBe(900);
    expect(effectiveTimeout(Number.NaN, 900)).toBe(900);
  });
});

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
    expect(tool.promptGuidelines.join('\n')).toContain('stops after at most 15m');
    expect(tool.promptGuidelines.join('\n')).toMatch(/background: true/);
  });

  it('runs a foreground command in the session cwd and returns its output', async () => {
    const { tool, ctx, cwd } = setup();
    const result = await tool.execute('t1', { command: 'pwd; echo hi' }, undefined, undefined, ctx);
    expect(result.content[0].text).toContain(fs.realpathSync(cwd));
    expect(result.content[0].text).toContain('hi');
  });

  it('stops a foreground command at its deadline and says how to run it longer', async () => {
    const { tool, ctx } = setup();
    const started = Date.now();
    await expect(tool.execute('t1', { command: 'echo before; sleep 30', timeout: 1 }, undefined, undefined, ctx)).rejects.toThrow(/before[\s\S]*Command timed out after 1 seconds[\s\S]*background: true/);
    expect(Date.now() - started).toBeLessThan(10_000);
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

  it('caps a background job log, keeping its latest output', async () => {
    const jobs = new BashJobs(tmp());
    let exited = false;
    const job = await jobs.start('i=0; while [ $i -lt 3000 ]; do echo "line $i padding padding padding"; i=$((i+1)); done', tmp(), undefined, () => (exited = true), { maxLogBytes: 8 * 1024 });
    await until(() => exited);
    const log = fs.readFileSync(job.log, 'utf8');
    expect(log.length).toBeLessThan(16 * 1024);
    expect(log).toMatch(/^\[earlier output dropped/);
    expect(log).toContain('line 2999');
    const small = path.join(tmp(), 'small.log');
    fs.writeFileSync(small, 'ok\n');
    capLog(small, 1024);
    expect(fs.readFileSync(small, 'utf8')).toBe('ok\n');
    capLog(path.join(tmp(), 'missing.log'));
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
    expect(sent[0]!.options).toEqual({ triggerTurn: true, deliverAs: 'followUp' });
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
    jobs.stopAll('session end');
    await Promise.all([...jobs.jobs.values()].map((job) => job.done));
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

  it('refuses a job past the running limit', async () => {
    const jobs = new BashJobs(tmp());
    const cwd = tmp();
    for (let index = 0; index < 8; index++) await jobs.start('sleep 30', cwd, undefined, () => undefined);
    await expect(jobs.start('sleep 30', cwd, undefined, () => undefined)).rejects.toThrow(/limit is 8/);
    jobs.stopAll('session end');
    await Promise.all([...jobs.jobs.values()].map((job) => job.done));
  });

  it('reports a finished job and cuts its log tail to whole lines', () => {
    const dir = tmp();
    const log = path.join(dir, 'job.log');
    fs.writeFileSync(log, `${'x'.repeat(5000)}\nlast line\n`);
    expect(logTail(log)).toBe('last line\n');
    expect(logTail(path.join(dir, 'missing.log'))).toBe('');
    const job = { id: 'bash-9', command: 'make', cwd: dir, pid: 1, log, startedAt: 0, done: Promise.resolve({ code: 0, signal: null, seconds: 75 }) };
    expect(jobReport(job, { code: 0, signal: null, seconds: 75 })).toBe(`Background bash bash-9 finished after 1m15s: make\nLog: job.log\nlast line`);
  });

  it('strips terminal escapes from the reported log tail', () => {
    const dir = tmp();
    const log = path.join(dir, 'job.log');
    fs.writeFileSync(log, '\u001b[32mPASS\u001b[0m tests\r\n\u001b]0;title\u0007done\n');
    const job = { id: 'bash-8', command: 'yarn test', cwd: dir, pid: 1, log, startedAt: 0, done: Promise.resolve({ code: 0, signal: null, seconds: 1 }) };
    expect(jobReport(job, { code: 0, signal: null, seconds: 1 }).split('\n').slice(2)).toEqual(['PASS tests', 'done']);
  });
});

describe('bash rendering', () => {
  const setup = () => {
    const { pi, tools, renderers } = fakePi();
    registerBashTool(pi, new Subcommands(), new BashJobs(tmp()));
    return { tool: tools.get('bash'), renderers };
  };
  const done = (extra: Record<string, unknown> = {}) => ({ lastComponent: undefined, isPartial: false, isError: false, state: {}, ...extra });

  it('heads the call with the command and shows a timeout only when the model set one', () => {
    const { tool } = setup();
    expect(rendered(tool.renderCall({ command: 'yarn test' }, theme, { lastComponent: undefined, isPartial: true, executionStarted: false }))).toBe('○ Bash(yarn test)');
    expect(rendered(tool.renderCall({ command: 'sleep 1', timeout: 30 }, theme, { lastComponent: undefined, isPartial: true }))).toBe('○ Bash(sleep 1) · timeout 30s');
    expect(rendered(tool.renderCall({ command: 'serve', background: true }, theme, done({ state: { durationMs: 40 } })))).toBe('● Bash(serve) · background · 40ms');
  });

  it('summarizes exit, line count and the last lines of output', () => {
    const { tool } = setup();
    const text = Array.from({ length: 12 }, (_, index) => `line ${index + 1}`).join('\n');
    const lines = rendered(tool.renderResult({ content: [{ type: 'text', text }], details: { durationMs: 5 } }, { expanded: false, isPartial: false }, theme, done())).split('\n');
    expect(lines).toEqual(['  ⎿  exit 0 · 12 lines', '     … +9 lines (ctrl+o to expand)', '     line 10', '     line 11', '     line 12']);
    const expanded = rendered(tool.renderResult({ content: [{ type: 'text', text }] }, { expanded: true, isPartial: false }, theme, done()));
    expect(expanded.split('\n')).toHaveLength(13);
    expect(rendered(tool.renderResult({ content: [{ type: 'text', text: '(no output)' }] }, { expanded: false, isPartial: false }, theme, done()))).toBe('  ⎿  exit 0 · no output');
  });

  it('draws failures as errors and keeps the spill path', () => {
    const { tool } = setup();
    const text = 'a\nb\n\n[Showing lines 1-2 of 900. Full output: /tmp/out.log]\n\nCommand exited with code 2';
    const lines = rendered(tool.renderResult({ content: [{ type: 'text', text }] }, { expanded: false, isPartial: false }, theme, done({ isError: true }))).split('\n');
    expect(lines).toEqual(['  ⎿  Error: exit 2 · 900 lines (ctrl+o to expand)', '     a', '     b']);
    expect(rendered(tool.renderResult({ content: [{ type: 'text', text }] }, { expanded: true, isPartial: false }, theme, done({ isError: true })))).toMatch(/\n {5}saved: \/tmp\/out\.log$/);
    const timeout = rendered(tool.renderResult({ content: [{ type: 'text', text: 'Command timed out after 120 seconds.\nIf it needs longer, rerun it with background: true and check its log.' }] }, { expanded: false, isPartial: false }, theme, done({ isError: true })));
    expect(timeout).toBe('  ⎿  Error: timed out after 2m · no output');
    const killed = rendered(tool.renderResult({ content: [{ type: 'text', text: 'build line\n\nCommand terminated without an exit code' }] }, { expanded: false, isPartial: false }, theme, done({ isError: true })));
    expect(killed.split('\n')[0]).toBe('  ⎿  Error: terminated (no exit code) · 1 line');
    expect(rendered(tool.renderResult({ content: [{ type: 'text', text: 'Working directory does not exist' }] }, { expanded: false, isPartial: false }, theme, done({ isError: true })))).toBe('  ⎿  Error: Working directory does not exist');
  });

  it('shows a started job and a running tail', () => {
    const { tool, renderers } = setup();
    expect(rendered(tool.renderResult({ content: [{ type: 'text', text: 'Started' }], details: { job: 'bash-1', pid: 42, log: '/tmp/j.log' } }, { expanded: false, isPartial: false }, theme, done()))).toBe('  ⎿  job bash-1 started (pid 42) (ctrl+o to expand)');
    expect(rendered(tool.renderResult({ content: [{ type: 'text', text: 'Started' }], details: { job: 'bash-1', pid: 42, log: '/tmp/j.log' } }, { expanded: true, isPartial: false }, theme, done()))).toBe('  ⎿  job bash-1 started (pid 42)\n     log: /tmp/j.log');
    expect(rendered(tool.renderResult({ content: [{ type: 'text', text: 'one\ntwo' }] }, { expanded: false, isPartial: true }, theme, { lastComponent: undefined, isPartial: true, executionStarted: true, state: {} }))).toBe('  ⎿  running · 2 lines\n     one\n     two');
    const job = [...renderers.values()][0];
    const message = { content: 'Background bash bash-1 finished after 3s: yarn build\nLog: /tmp/j.log\nok' };
    expect(rendered(job(message, { expanded: false }, theme))).toBe(' ● Job(bash-1: yarn build)\n   ⎿  finished after 3s (ctrl+o to expand)\n      ok');
    expect(rendered(job(message, { expanded: true }, theme))).toBe(' ● Job(bash-1: yarn build)\n   ⎿  finished after 3s\n      ok\n      log: /tmp/j.log');
    // A job the user killed is interrupted, not failed.
    const colored = { ...(theme as object), fg: (color: string, text: string) => `<${color}>${text}` };
    const killed = rendered(job({ content: 'Background bash bash-1 stopped (user) after 3s: yarn build\nLog: /tmp/j.log\n(no output)' }, { expanded: false }, colored));
    expect(killed).toContain('<warning>◼');
    expect(killed).not.toContain('<error>');
  });
});
