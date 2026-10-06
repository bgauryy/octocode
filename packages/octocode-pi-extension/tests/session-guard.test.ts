import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import octocode from '../src/index.js';
import { fakeCtx, fakePi } from './fake-pi.js';
import { tmp } from './helpers.js';

let saved: NodeJS.ProcessEnv;
beforeEach(() => {
  saved = { ...process.env };
  process.env['OCTOCODE_HOME'] = tmp();
});
afterEach(() => {
  process.env = saved;
});

/** The composed extension on a fake Pi, with a TUI session in a trusted-looking temp project. */
function setup(options: { hasUI?: boolean } = {}) {
  const fake = fakePi();
  octocode(fake.pi);
  const cwd = tmp();
  const ctx = fakeCtx({ cwd, hasUI: options.hasUI ?? true });
  Object.assign(ctx, { isProjectTrusted: () => false });
  return { fake, ctx, cwd };
}

/** A session_before_* outcome as Pi's runner decides it: the first cancel wins, else the last defined result. */
async function decide(fake: ReturnType<typeof fakePi>, event: string, payload: unknown, ctx: unknown): Promise<unknown> {
  let result: unknown;
  for (const handler of fake.handlers.get(event) ?? []) {
    const each = (await handler(payload, ctx)) as { cancel?: boolean } | undefined;
    if (each?.cancel) return each;
    result = each ?? result;
  }
  return result;
}

describe('leaving a session with running work', () => {
  it('asks before a switch, new session or fork stops running jobs; No cancels, headless never asks', async () => {
    const { fake, ctx, cwd } = setup();
    // Nothing running: no question.
    expect(await decide(fake, 'session_before_switch', { type: 'session_before_switch', reason: 'resume' }, ctx)).toBeUndefined();
    expect(ctx.ui.confirms).toEqual([]);

    await fake.tools.get('bash').execute('t1', { command: 'sleep 30', background: true }, undefined, undefined, ctx);
    try {
      let asked = '';
      Object.assign(ctx.ui, { confirm: async (_title: string, message: string) => ((asked = message), ctx.ui.confirms.shift() ?? false) });
      ctx.ui.confirms.push(false);
      expect(await decide(fake, 'session_before_switch', { type: 'session_before_switch', reason: 'new' }, ctx)).toEqual({ cancel: true });
      expect(asked).toBe('Stop 1 bash job and start a new session?');
      ctx.ui.confirms.push(false);
      // A No cancels the fork (with no checkpoints here, this does not exercise the restore offer).
      expect(await decide(fake, 'session_before_fork', { type: 'session_before_fork', entryId: 'e1' }, ctx)).toEqual({ cancel: true });
      expect(asked).toBe('Stop 1 bash job and fork?');
      ctx.ui.confirms.push(true);
      expect(await decide(fake, 'session_before_switch', { type: 'session_before_switch', reason: 'resume' }, ctx)).toBeUndefined();
      expect(asked).toBe('Stop 1 bash job and switch sessions?');

      const headless = fakeCtx({ cwd, hasUI: false });
      expect(await decide(fake, 'session_before_switch', { type: 'session_before_switch', reason: 'new' }, headless)).toBeUndefined();
    } finally {
      await fake.emit('session_shutdown', { type: 'session_shutdown' }, ctx);
    }
  });
});

describe('user ! commands', () => {
  const bash = (command: string) => ({ type: 'user_bash', command, excludeFromContext: false, cwd: '/' });

  it('runs the bash safety gate: refused headless or on No, run on Yes', async () => {
    const { fake, ctx } = setup();
    expect(await fake.fire('user_bash', bash('echo fine'), ctx)).toBeUndefined();

    const refused = (await fake.fire('user_bash', bash('rm -rf /'), ctx)) as { result: { output: string; cancelled: boolean } };
    expect(refused.result.cancelled).toBe(true);
    expect(refused.result.output).toMatch(/^Refused: /);
    ctx.ui.confirms.push(true);
    expect(await fake.fire('user_bash', bash('rm -rf /'), ctx)).toBeUndefined();

    const headless = setup({ hasUI: false });
    expect(((await headless.fake.fire('user_bash', bash('rm -rf /'), headless.ctx)) as { result: { output: string } }).result.output).toMatch(/^Refused: /);
  });
});
