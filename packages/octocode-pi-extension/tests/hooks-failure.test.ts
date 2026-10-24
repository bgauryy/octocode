import path from 'node:path';
import fs from 'node:fs';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { commandsFor, parseHooks } from '../src/hooks/config.js';
import { registerHooks } from '../src/hooks/register.js';
import { failureOf } from '../src/hooks/runner.js';
import { fingerprint, projectConfigFiles, saveDecision } from '../src/shared/trust.js';
import { fakePi } from './fake-pi.js';
import { repo, tmp } from './helpers.js';

beforeEach(() => {
  vi.stubEnv('OCTOCODE_HOME', tmp('octocode-hooks-home-'));
});

type Hook = { type: 'command'; command: string; timeout?: number; failClosed?: boolean };

/** A trusted project with one PreToolUse hook for Bash. */
function project(hook: Hook): string {
  const dir = repo('octocode-hookfail-');
  fs.mkdirSync(path.join(dir, '.codex'));
  fs.writeFileSync(path.join(dir, '.codex', 'hooks.json'), JSON.stringify({ hooks: { PreToolUse: [{ matcher: 'Bash', hooks: [hook] }] } }));
  saveDecision(dir, fingerprint(projectConfigFiles(dir)), true);
  return dir;
}

async function run(hook: Hook, options: { hasUI?: boolean } = {}) {
  const cwd = project(hook);
  const { pi, fire } = fakePi();
  const failures: Array<{ what: readonly string[]; note: string; allowed: boolean }> = [];
  const hooks = registerHooks(pi, { OCTOCODE_HOOKS: '1' }, tmp(), (what, note, allowed) => failures.push({ what, note, allowed }));
  const notify = vi.fn();
  const ctx = { cwd, hasUI: options.hasUI ?? true, ui: { notify, setStatus: () => undefined }, isProjectTrusted: () => true, sessionManager: { getSessionId: () => 's', getSessionFile: () => undefined } };
  await fire('session_start', { reason: 'startup' }, ctx);
  const call = () => hooks.preToolUse({ toolName: 'bash', toolCallId: 'b', input: { command: 'ls' } } as never, ctx as never);
  return { call, failures, notify, fire, ctx };
}

const TIMEOUT: Hook = { type: 'command', command: 'sleep 5', timeout: 0.2 };

describe('PreToolUse hook failures', () => {
  it('warns, audits and lets the call go ahead when a hook times out (fail open)', async () => {
    const { call, failures, notify } = await run(TIMEOUT);
    expect(await call()).toBeUndefined();
    expect(notify).toHaveBeenCalledTimes(1);
    expect(notify.mock.calls[0]).toEqual([expect.stringMatching(/PreToolUse hook timed out after 0\.2s: sleep 5.*allowed.*failClosed/), 'warning']);
    expect(failures).toEqual([{ what: ['PreToolUse hook timed out after 0.2s: sleep 5', 'tool: Bash'], note: expect.stringContaining('fail open'), allowed: true }]);
    // The warning is shown once per hook; every failure is audited.
    await call();
    expect(notify).toHaveBeenCalledTimes(1);
    expect(failures).toHaveLength(2);
  }, 15_000);

  it('shows controls in a failed hook\'s command as visible escapes in the reason the model reads', async () => {
    const { call } = await run({ ...TIMEOUT, command: 'sleep 5 # \u001b[31m\u202e', failClosed: true });
    const verdict = (await call()) as { reason: string };
    expect(verdict.reason).toContain('failClosed: sleep 5 # \\u{1b}[31m\\u{202e}');
    expect(verdict.reason).not.toMatch(/[\u001b\u202e]/u);
  }, 15_000);

  it('denies the call when a failClosed hook times out, and says so', async () => {
    const { call, failures, notify } = await run({ ...TIMEOUT, failClosed: true });
    expect(await call()).toEqual({ block: true, reason: expect.stringMatching(/^Blocked by a PreToolUse hook: PreToolUse hook timed out after 0\.2s and is set to failClosed: sleep 5/) });
    expect(notify.mock.calls[0]?.[0]).toMatch(/denied \(failClosed\)/);
    expect(failures).toEqual([expect.objectContaining({ allowed: false, note: expect.stringContaining('failClosed denied') })]);
  }, 15_000);

  it('treats a crash and invalid JSON as failures, headless too (audit only), and a clean answer or exit 2 as none', async () => {
    const crash = await run({ type: 'command', command: 'exit 3', failClosed: true }, { hasUI: false });
    expect(await crash.call()).toMatchObject({ block: true });
    expect(crash.notify).not.toHaveBeenCalled();
    expect(crash.failures[0]?.what[0]).toBe('PreToolUse hook crashed (exit 3): exit 3');
    const invalid = await run({ type: 'command', command: 'echo "{oops"' });
    expect(await invalid.call()).toBeUndefined();
    expect(invalid.failures[0]?.what[0]).toBe('PreToolUse hook printed invalid JSON: echo "{oops"');
    for (const command of ['exit 0', 'echo checked 3 files', `echo '{"hookSpecificOutput":{"permissionDecision":"ask"}}'`]) {
      const fine = await run({ type: 'command', command, failClosed: true });
      expect(await fine.call(), command).not.toMatchObject({ block: true });
      expect(fine.failures, command).toEqual([]);
    }
    const blocks = await run({ type: 'command', command: 'echo no >&2; exit 2', failClosed: true });
    expect(await blocks.call()).toEqual({ block: true, reason: 'Blocked by a PreToolUse hook: no' });
    expect(blocks.failures).toEqual([]);
  }, 20_000);

  it('does not treat other events or a cancelled run as failures', () => {
    const base = { code: null, stdout: '', stderr: '', timedOut: false };
    expect(failureOf({ timeoutMs: 1000 }, base, true)).toBeUndefined();
    expect(failureOf({ timeoutMs: 1000 }, base, false)).toMatch(/^crashed/);
    expect(failureOf({ timeoutMs: 1000 }, { ...base, code: 0 }, false)).toBeUndefined();
    expect(failureOf({ timeoutMs: 2500 }, { ...base, timedOut: true }, false)).toBe('timed out after 2.5s');
  });

  it('reads failClosed from the config, and keeps it when the same command is configured twice', () => {
    const config = parseHooks({ hooks: { PreToolUse: [{ hooks: [{ type: 'command', command: 'g', failClosed: 'yes' }, { type: 'command', command: 'h', failClosed: true }] }] } }, 'a');
    parseHooks({ hooks: { PreToolUse: [{ hooks: [{ type: 'command', command: 'g', failClosed: true }] }] } }, 'b', config);
    expect(commandsFor(config, 'PreToolUse', ['bash'])).toEqual([
      expect.objectContaining({ command: 'g', failClosed: true }),
      expect.objectContaining({ command: 'h', failClosed: true }),
    ]);
    expect(parseHooks({ hooks: { PreToolUse: [{ hooks: [{ type: 'command', command: 'g', failClosed: 'yes' }] }] } }, 'a').PreToolUse[0]!.hooks[0]).not.toHaveProperty('failClosed');
  });
});

describe('observer hook dispatch failures', () => {
  /** A trusted project with `event` hooks; the session id lookup throws after `okCalls` calls, which fails the dispatch itself. */
  async function failing(event: 'Notification' | 'PostMessageReceive', okCalls: number) {
    const cwd = tmp('octocode-hookobs-');
    fs.mkdirSync(path.join(cwd, '.git'));
    fs.mkdirSync(path.join(cwd, '.codex'));
    fs.writeFileSync(path.join(cwd, '.codex', 'hooks.json'), JSON.stringify({ hooks: { [event]: [{ hooks: [{ type: 'command', command: 'true' }] }] } }));
    saveDecision(cwd, fingerprint(projectConfigFiles(cwd)), true);
    const { pi, fire } = fakePi();
    const hooks = registerHooks(pi, { OCTOCODE_HOOKS: '1' }, tmp());
    const notify = vi.fn();
    let calls = 0;
    const ctx = {
      cwd,
      hasUI: true,
      ui: { notify, setStatus: () => undefined },
      isProjectTrusted: () => true,
      sessionManager: {
        getSessionId: () => {
          if (++calls > okCalls) throw new Error('session store unavailable');
          return 's';
        },
        getSessionFile: () => undefined,
      },
    };
    await fire('session_start', { reason: 'startup' }, ctx);
    calls = 0;
    return { hooks, notify, ctx };
  }

  it('warns once when a Notification hook cannot be dispatched', async () => {
    const { hooks, notify, ctx } = await failing('Notification', 0);
    hooks.notification({ type: 'idle_prompt', message: 'idle' }, ctx as never);
    await vi.waitFor(() => expect(notify).toHaveBeenCalledTimes(1));
    expect(notify).toHaveBeenCalledWith('Notification hooks could not run: session store unavailable', 'warning');
    hooks.notification({ type: 'idle_prompt', message: 'idle' }, ctx as never);
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(notify).toHaveBeenCalledTimes(1);
  });

  it('warns when a PostMessageReceive hook cannot be dispatched', async () => {
    // The first lookup (is the message for this session?) works; the dispatch's own lookup fails.
    const { hooks, notify, ctx } = await failing('PostMessageReceive', 1);
    await hooks.messageReceive({ channel: 'internal', direction: 'inbound', from: 'a', to: 'b', text: 'hi', id: 'm-1' }, ctx as never);
    expect(notify).toHaveBeenCalledWith('PostMessageReceive hooks could not run: session store unavailable', 'warning');
  });
});
