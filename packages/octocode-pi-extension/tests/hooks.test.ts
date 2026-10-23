import fs from 'node:fs';
import path from 'node:path';
import { describe, expect, it, vi } from 'vitest';
import { registerHooks, registerHooksCommand } from '../src/hooks/register.js';
import { Subcommands } from '../src/shared/commands.js';
import { fakePi, rendered, theme } from './fake-pi.js';
import { tmp } from './helpers.js';
import { command, project, useHooksHome } from './hooks-harness.js';

useHooksHome();

const context = (cwd: string, trusted = true) => ({ cwd, hasUI: false, isProjectTrusted: () => trusted, sessionManager: { getSessionId: () => 'sess-1', getSessionFile: () => undefined } });

describe('registerHooks', () => {
  it('blocks a file call when an Edit|Write guard exits 2 on one of its queries', async () => {
    // A typical Claude Code guard: refuse any edit or write to a .env file.
    const guard = `node -e "let s='';process.stdin.on('data',d=>s+=d).on('end',()=>{const p=JSON.parse(s).tool_input.file_path||'';if(p.endsWith('.env')){console.error('no .env: '+p);process.exit(2)}})"`;
    const cwd = project({ PreToolUse: command(guard, 'Edit|Write') });
    const { pi, fire } = fakePi();
    const hooks = registerHooks(pi, { OCTOCODE_HOOKS: '1' }, tmp());
    const ctx = context(cwd);
    await fire('session_start', { reason: 'startup' }, ctx);
    const call = (...paths: string[]) => ({ toolName: 'file', toolCallId: 'f', input: { queries: paths.map((p) => ({ reasoning: 'r', type: 'write', path: p, content: 'x' })) } });
    expect(await hooks.preToolUse(call('a.ts') as never, ctx as never)).toBeUndefined();
    const blocked = await hooks.preToolUse(call('a.ts', '.env') as never, ctx as never);
    expect(blocked).toEqual({ block: true, reason: `Blocked by a PreToolUse hook: no .env: ${path.join(cwd, '.env')}` });
  }, 10_000);

  it('shows a status while the first prompt waits for SessionStart hooks', async () => {
    const cwd = project({ SessionStart: command('while [ ! -f go ]; do sleep 0.02; done; echo ready') });
    const { pi, fire } = fakePi();
    registerHooks(pi, { OCTOCODE_HOOKS: '1' }, tmp());
    const statuses: Array<string | undefined> = [];
    const ctx = { ...context(cwd), hasUI: true, ui: { notify: () => undefined, setStatus: (_key: string, text: string | undefined) => statuses.push(text) } };
    await fire('session_start', { reason: 'startup' }, ctx);
    const waiting = fire('before_agent_start', {}, ctx);
    await vi.waitFor(() => expect(statuses).toEqual(['Waiting for SessionStart hooks…']));
    fs.writeFileSync(path.join(cwd, 'go'), '');
    expect(((await waiting) as { message: { content: string } }).message.content).toContain('ready');
    expect(statuses).toEqual(['Waiting for SessionStart hooks…', undefined]);
    // Later prompts do not wait or flash the status.
    await fire('before_agent_start', {}, ctx);
    expect(statuses).toHaveLength(2);
    await fire('session_shutdown', {}, ctx);
  }, 10_000);

  it('is off unless OCTOCODE_HOOKS is set', () => {
    const { pi, handlers } = fakePi();
    registerHooks(pi, {});
    expect(handlers.size).toBe(0);
  });

  it('maps PreToolUse, PostToolUse, SessionStart and PreCompact to Pi events', async () => {
    const cwd = project({
      PreToolUse: command('cat > pre.json; echo "blocked rm" >&2; exit 2', 'Bash'),
      PostToolUse: command('echo \'{"hookSpecificOutput":{"additionalContext":"lint ok"}}\'', 'Edit'),
      SessionStart: command('echo "repo notes"', 'startup'),
      PreCompact: command('cat > compact.json', 'manual'),
    });
    const { pi, fire } = fakePi();
    const hooks = registerHooks(pi, { OCTOCODE_HOOKS: '1' }, tmp());
    const ctx = context(cwd);
    await fire('session_start', { type: 'session_start', reason: 'startup' }, ctx);

    const message = (await fire('before_agent_start', {}, ctx)) as { message: { content: string; display: boolean } };
    expect(message.message.content).toContain('repo notes');
    expect(message.message.display).toBe(false);
    expect(await fire('before_agent_start', {}, ctx)).toBeUndefined();

    const blocked = await hooks.preToolUse({ toolName: 'bash', toolCallId: '1', input: { command: 'rm -rf /' } } as never, ctx as never);
    expect(blocked).toEqual({ block: true, reason: 'Blocked by a PreToolUse hook: blocked rm' });
    expect(JSON.parse(fs.readFileSync(path.join(cwd, 'pre.json'), 'utf8'))).toMatchObject({ session_id: 'sess-1', hook_event_name: 'PreToolUse', tool_name: 'Bash', pi_tool_name: 'bash', tool_input: { command: 'rm -rf /' } });
    expect(await hooks.preToolUse({ toolName: 'read', toolCallId: '2', input: {} } as never, ctx as never)).toBeUndefined();

    const edit = { queries: [{ reasoning: 'r', type: 'edit', path: 'a.ts', edits: [{ oldText: 'a', newText: 'b' }] }] };
    const result = (await fire('tool_result', { toolName: 'file', toolCallId: '3', input: edit, content: [{ type: 'text', text: 'edited' }], isError: false }, ctx)) as { content: Array<{ text: string }> };
    expect(result.content.map((part) => part.text)).toEqual(['edited', '\n[PostToolUse hook] lint ok']);

    await fire('session_before_compact', { reason: 'manual', customInstructions: 'keep tests' }, ctx);
    expect(JSON.parse(fs.readFileSync(path.join(cwd, 'compact.json'), 'utf8'))).toMatchObject({ hook_event_name: 'PreCompact', trigger: 'manual', custom_instructions: 'keep tests' });
  });

  it('runs SessionStart hooks without holding up session start, and drops a replaced session\'s result', async () => {
    // The hook echoes the start source it was given, after a delay.
    // The hook waits for a `go` file the test writes after both session starts returned.
    const cwd = project({ SessionStart: command(`while [ ! -f go ]; do sleep 0.02; done; node -e "let s='';process.stdin.on('data',d=>s+=d).on('end',()=>console.log('source=' + JSON.parse(s).source))"`) });
    const { pi, fire } = fakePi();
    registerHooks(pi, { OCTOCODE_HOOKS: '1' }, tmp());
    const ctx = context(cwd);
    await fire('session_start', { type: 'session_start', reason: 'startup' }, ctx);
    // A second session starts before the first one's hook finished: only the new session's context reaches its prompt.
    await fire('session_start', { type: 'session_start', reason: 'resume' }, ctx);
    fs.writeFileSync(path.join(cwd, 'go'), '');
    const message = (await fire('before_agent_start', {}, ctx)) as { message: { content: string } };
    expect(message.message.content).toContain('source=resume');
    expect(message.message.content).not.toContain('source=startup');
    await fire('session_shutdown', {}, ctx);
  }, 10_000);

  it('runs SessionStart hooks again after a compaction (source compact), and their context comes with the next prompt', async () => {
    const cwd = project({ SessionStart: command(`node -e "let s='';process.stdin.on('data',d=>s+=d).on('end',()=>console.log('source=' + JSON.parse(s).source))"`, 'startup|compact') });
    const { pi, fire } = fakePi();
    registerHooks(pi, { OCTOCODE_HOOKS: '1' }, tmp());
    const ctx = context(cwd);
    await fire('session_start', { type: 'session_start', reason: 'startup' }, ctx);
    expect(((await fire('before_agent_start', {}, ctx)) as { message: { content: string } }).message.content).toContain('source=startup');
    await fire('session_compact', { type: 'session_compact' }, ctx);
    const message = (await fire('before_agent_start', {}, ctx)) as { message: { content: string } };
    expect(message.message.content).toContain('source=compact');
    expect(message.message.content).not.toContain('source=startup');
    await fire('session_shutdown', {}, ctx);
  }, 10_000);

  it('reports hooks off with the files it would read, and on with what loaded', async () => {
    const cwd = project({ PreToolUse: command('echo pre', 'Bash') });
    const home = tmp();
    const off = registerHooks(fakePi().pi, {}, home);
    expect(off.summary()).toBe('off (OCTOCODE_HOOKS=1 to enable)');
    expect(off.report(cwd)).toMatch(/^Hooks: off\. Start Pi with OCTOCODE_HOOKS=1/);
    expect(off.report(cwd)).toContain(`· ~${path.sep}${path.join('.claude', 'settings.json')}`);
    expect(off.report(cwd)).toMatch(/✓ .*hooks\.json \(project: loads only when trusted/);

    const { pi, fire } = fakePi();
    const on = registerHooks(pi, { OCTOCODE_HOOKS: '1' }, home);
    await fire('session_start', { reason: 'startup' }, context(cwd));
    expect(on.summary()).toBe('on, 1 commands');
    expect(on.report(cwd)).toMatch(/Loaded \(1\):\n  PreToolUse \[Bash\] echo pre \(.*hooks\.json\)/);

    const commands = new Subcommands();
    registerHooksCommand(commands, on);
    const notes: string[] = [];
    await commands.get('hooks')!.handler('', { cwd, ui: { notify: (text: string) => notes.push(text) } } as never);
    expect(notes[0]).toBe(on.report(cwd));
  });

  it('warns once when SessionStart hooks cannot run', async () => {
    const cwd = project({ SessionStart: command('echo hi') });
    const { pi, fire } = fakePi();
    registerHooks(pi, { OCTOCODE_HOOKS: '1' }, tmp());
    const notify = vi.fn();
    const broken = { ...context(cwd), hasUI: true, ui: { notify, setStatus: () => undefined }, sessionManager: { getSessionId: () => { throw new Error('no session'); }, getSessionFile: () => undefined } };
    await fire('session_start', { reason: 'startup' }, broken);
    expect(await fire('before_agent_start', {}, broken)).toBeUndefined();
    await fire('session_start', { reason: 'startup' }, broken);
    await fire('before_agent_start', {}, broken);
    const warnings = notify.mock.calls.filter(([, level]) => level === 'warning');
    expect(warnings).toHaveLength(1);
    expect(String(warnings[0]![0])).toContain('no session');
  });

  it('skips project hooks in an untrusted project', async () => {
    const cwd = project({ PreToolUse: command('exit 2') });
    const { pi, fire } = fakePi();
    const hooks = registerHooks(pi, { OCTOCODE_HOOKS: '1' }, tmp());
    await fire('session_start', { reason: 'startup' }, context(cwd, false));
    expect(await hooks.preToolUse({ toolName: 'bash', input: {} } as never, context(cwd, false) as never)).toBeUndefined();
  });
});

describe('Stop, Notification and hook timing', () => {
  const settle = (outcome = 'completed') => ({ type: 'agent_before_settle', outcome, entries: [], continue: false, context: { llmMessages: [{ role: 'assistant', content: [{ type: 'text', text: 'final answer' }] }] } });

  it('continues a completed run once a Stop hook blocks, with Claude Code loop guards', async () => {
    const stop = `node -e "let s='';process.stdin.on('data',d=>s+=d).on('end',()=>{const i=JSON.parse(s);require('fs').appendFileSync('stop.log',JSON.stringify(i)+'\\n');if(!i.stop_hook_active)console.log(JSON.stringify({decision:'block',reason:'run the tests'}))})"`;
    const cwd = project({ Stop: command(stop) });
    const { pi, fire } = fakePi();
    registerHooks(pi, { OCTOCODE_HOOKS: '1' }, tmp());
    const ctx = context(cwd);
    await fire('session_start', { reason: 'startup' }, ctx);
    expect(await fire('agent_before_settle', settle('aborted'), ctx)).toBeUndefined();
    const first = (await fire('agent_before_settle', settle(), ctx)) as { entries: Array<{ content: string; customType: string }>; continue: boolean };
    expect(first.continue).toBe(true);
    expect(first.entries[0]).toMatchObject({ type: 'custom_message', customType: 'octocode-hook-stop', content: 'Stop hook feedback:\nrun the tests', display: true });
    expect(await fire('agent_before_settle', settle(), ctx)).toBeUndefined();
    const inputs = fs.readFileSync(path.join(cwd, 'stop.log'), 'utf8').trim().split('\n').map((line) => JSON.parse(line));
    expect(inputs.map((input) => input.stop_hook_active)).toEqual([false, true]);
    expect(inputs[0]).toMatchObject({ hook_event_name: 'Stop', last_assistant_message: 'final answer' });
    // A prompt resets the guard.
    await fire('before_agent_start', {}, ctx);
    // Pi replaces the boundary's entries with a handler's: entries earlier handlers proposed are kept.
    const earlier = { type: 'custom', customType: 'other-extension', data: 1 };
    const next = (await fire('agent_before_settle', { ...settle(), entries: [earlier] }, ctx)) as { entries: unknown[]; continue: boolean };
    expect(next.continue).toBe(true);
    expect(next.entries[0]).toEqual(earlier);
    expect(next.entries).toHaveLength(2);
  }, 10_000);

  it('shows the running Stop hooks in the footer status (Pi has removed its working line), then clears it', async () => {
    const cwd = project({ Stop: [{ matcher: '', hooks: [{ type: 'command', command: 'node -e "setTimeout(Boolean,1300)" #ghp_abcdefghijklmnopqrstuvwxyz0123456789' }, { type: 'command', command: 'true' }] }] });
    const { pi, fire } = fakePi();
    registerHooks(pi, { OCTOCODE_HOOKS: '1' }, tmp());
    const statuses: Array<string | undefined> = [];
    const ctx = { ...context(cwd), hasUI: true, mode: 'tui', ui: { setStatus: (_key: string, text: string | undefined) => statuses.push(text), notify: () => undefined } };
    await fire('session_start', { reason: 'startup' }, ctx);
    expect(await fire('agent_before_settle', settle(), ctx)).toBeUndefined();
    expect(statuses[0]).toMatch(/^Stop hook: node -e .*\[redacted\] \+1$/);
    expect(statuses.some((text) => / · 1(\.\d)?s$/.test(text ?? ''))).toBe(true);
    expect(statuses.at(-1)).toBeUndefined();
  }, 10_000);

  it('draws Stop feedback as a sanitized, collapsible block under a ⚑ header, even with hooks off', () => {
    const { pi, renderers } = fakePi();
    registerHooks(pi, {}, tmp());
    const render = renderers.get('octocode-hook-stop');
    const content = 'Stop hook feedback:\n# not a heading\n\u001b]52;c;evil\u0007two\nthree\nfour\nfive';
    const collapsed = rendered(render({ content }, { expanded: false }, theme));
    expect(collapsed).toMatch(/^\s*⚑ Stop hook/);
    expect(collapsed).not.toContain('Stop hook feedback:');
    expect(collapsed).toContain('# not a heading');
    expect(collapsed).not.toMatch(/\u001b\]52|\u0007/);
    expect(collapsed).not.toContain('five');
    expect(collapsed).toMatch(/… \+2 lines \(ctrl\+o to expand\)/);
    expect(rendered(render({ content }, { expanded: true }, theme))).toContain('five');
  });

  it('stops continuing after 8 blocks in a row unless a tool runs; continue:false is not a block', async () => {
    const cwd = project({ Stop: command('echo \'{"decision":"block","reason":"again"}\'') });
    const { pi, fire } = fakePi();
    registerHooks(pi, { OCTOCODE_HOOKS: '1' }, tmp());
    const ctx = context(cwd);
    await fire('session_start', { reason: 'startup' }, ctx);
    for (let index = 0; index < 8; index += 1) expect(await fire('agent_before_settle', settle(), ctx)).toBeDefined();
    expect(await fire('agent_before_settle', settle(), ctx)).toBeUndefined();
    await fire('tool_execution_start', { toolCallId: 'n', toolName: 'x', parentToolCallId: 'p' }, ctx);
    expect(await fire('agent_before_settle', settle(), ctx)).toBeUndefined();
    await fire('tool_execution_start', { toolCallId: 't', toolName: 'x' }, ctx);
    expect(await fire('agent_before_settle', settle(), ctx)).toBeDefined();

    const quiet = project({ Stop: command('echo \'{"continue":false,"stopReason":"done"}\'') });
    const other = fakePi();
    registerHooks(other.pi, { OCTOCODE_HOOKS: '1' }, tmp());
    await other.fire('session_start', { reason: 'startup' }, context(quiet));
    expect(await other.fire('agent_before_settle', settle(), context(quiet))).toBeUndefined();
  }, 20_000);

  it('runs Notification hooks matching the type, in the background', async () => {
    const cwd = project({ Notification: command('cat > note.json', 'permission_prompt') });
    const { pi, fire } = fakePi();
    const hooks = registerHooks(pi, { OCTOCODE_HOOKS: '1' }, tmp());
    await fire('session_start', { reason: 'startup' }, context(cwd));
    // Background: an aborted run (its signal) does not cancel the hook.
    const aborted = { ...context(cwd), signal: AbortSignal.abort() };
    hooks.notification({ type: 'idle_prompt', message: 'idle' }, aborted as never);
    hooks.notification({ type: 'permission_prompt', message: 'Save?', title: 'Pi: waiting for you' }, aborted as never);
    await vi.waitFor(() => expect(fs.existsSync(path.join(cwd, 'note.json')) && fs.readFileSync(path.join(cwd, 'note.json'), 'utf8')).toBeTruthy());
    expect(JSON.parse(fs.readFileSync(path.join(cwd, 'note.json'), 'utf8'))).toMatchObject({ hook_event_name: 'Notification', message: 'Save?', title: 'Pi: waiting for you', notification_type: 'permission_prompt' });
  }, 10_000);

  it('runs matching hooks in parallel, once per command, and reports their timing', async () => {
    const slow = 'sleep 0.4';
    const cwd = project({ PreToolUse: [{ matcher: 'Bash', hooks: [{ type: 'command', command: slow }, { type: 'command', command: 'sleep 0.41' }, { type: 'command', command: slow }] }] });
    const { pi, fire } = fakePi();
    const hooks = registerHooks(pi, { OCTOCODE_HOOKS: '1' }, tmp());
    await fire('session_start', { reason: 'startup' }, context(cwd));
    const started = Date.now();
    expect(await hooks.preToolUse({ toolName: 'bash', toolCallId: '1', input: { command: 'ls' } } as never, context(cwd) as never)).toBeUndefined();
    expect(Date.now() - started).toBeLessThan(750);
    const report = hooks.report(cwd);
    expect(report).toMatch(/sleep 0\.4 \(.*\) · 1 run, avg \d/);
    expect(report).toMatch(/sleep 0\.41 \(.*\) · 1 run/);
  }, 10_000);

  it('warns once about a slow PreToolUse hook', async () => {
    vi.useFakeTimers({ toFake: ['Date'] });
    try {
      const cwd = project({ PreToolUse: command('true', 'Bash') });
      const { pi, fire } = fakePi();
      const hooks = registerHooks(pi, { OCTOCODE_HOOKS: '1' }, tmp());
      const notes: string[] = [];
      const ctx = { ...context(cwd), hasUI: true, ui: { notify: (message: string) => notes.push(message), setStatus: () => undefined } };
      await fire('session_start', { reason: 'startup' }, ctx);
      const call = () => {
        const done = hooks.preToolUse({ toolName: 'bash', toolCallId: '1', input: { command: 'ls' } } as never, ctx as never);
        vi.setSystemTime(Date.now() + 6_000);
        return done;
      };
      await call();
      await call();
      expect(notes).toHaveLength(1);
      expect(notes[0]).toMatch(/^Slow PreToolUse hook \(6s\): every tool call in a batch waits for it\. true$/);
    } finally {
      vi.useRealTimers();
    }
  }, 10_000);
});
