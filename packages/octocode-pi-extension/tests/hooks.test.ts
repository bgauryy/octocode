import fs from 'node:fs';
import path from 'node:path';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { commandsFor, hookCalls, hookFiles, loadHooks, matches, parseHooks, toolAliases } from '../src/hooks/config.js';
import { registerHooks, registerHooksCommand } from '../src/hooks/register.js';
import { Subcommands } from '../src/shared/commands.js';
import { capContext, HOOK_CONTEXT_MAX_BYTES, interpret, runHook } from '../src/hooks/runner.js';
import { fakePi, rendered, theme } from './fake-pi.js';
import { tmp } from './helpers.js';

function project(hooks: unknown, file = path.join('.codex', 'hooks.json')): string {
  const dir = tmp('octocode-hooks-');
  fs.mkdirSync(path.join(dir, '.git'));
  fs.mkdirSync(path.dirname(path.join(dir, file)), { recursive: true });
  fs.writeFileSync(path.join(dir, file), JSON.stringify({ hooks }));
  // A Pi-protected resource: Pi itself resolved trust for this project, so Octocode asks nothing more.
  fs.mkdirSync(path.join(dir, '.pi'), { recursive: true });
  fs.writeFileSync(path.join(dir, '.pi', 'settings.json'), '{}');
  return dir;
}

// Pi-protected projects record their first trust decision: keep it out of the developer's own store.
beforeEach(() => {
  vi.stubEnv('OCTOCODE_HOME', tmp('octocode-hooks-home-'));
});

/** Whether a process is still running. */
function alive(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

const context = (cwd: string, trusted = true) => ({ cwd, hasUI: false, isProjectTrusted: () => trusted, sessionManager: { getSessionId: () => 'sess-1', getSessionFile: () => undefined } });
const command = (text: string, matcher = '') => [{ matcher, hooks: [{ type: 'command', command: text }] }];

describe('hooks config', () => {
  it('reads project hook files only in trusted projects', () => {
    const cwd = project({ PreToolUse: command('true') });
    const home = tmp();
    // ~/.claude/settings.json, ~/.codex/hooks.json, ~/.octocode/hooks.json
    expect(hookFiles(path.join(cwd), false, home)).toHaveLength(3);
    expect(loadHooks(cwd, false, home).config.PreToolUse).toHaveLength(0);
    expect(loadHooks(cwd, true, home).config.PreToolUse).toHaveLength(1);
  });

  it('keeps command hooks for mapped events and reports invalid JSON', () => {
    const config = parseHooks({ hooks: { PreToolUse: [{ matcher: 'Bash', hooks: [{ type: 'command', command: 'x', timeout: 5 }, { type: 'prompt', prompt: 'p' }] }], Stop: command('y') } }, 'f');
    expect(config.PreToolUse).toEqual([{ matcher: 'Bash', hooks: [{ command: 'x', timeoutMs: 5000, source: 'f' }] }]);
    const home = tmp();
    fs.mkdirSync(path.join(home, '.claude'));
    fs.writeFileSync(path.join(home, '.claude', 'settings.json'), '{oops');
    expect(loadHooks(tmp(), false, home).errors[0]).toContain('settings.json');
  });

  it('matches Claude tool names against Pi tools', () => {
    expect(matches('Bash', toolAliases('bash'))).toBe(true);
    expect(matches('Edit|Write', toolAliases('file'))).toBe(true);
    expect(matches('', ['anything'])).toBe(true);
    expect(matches('*', ['anything'])).toBe(true);
    expect(matches('Read', toolAliases('bash'))).toBe(false);
    expect(matches('(', ['x'])).toBe(false);
    expect(commandsFor(parseHooks({ hooks: { PreToolUse: command('a', 'Bash') } }, 'f'), 'PreToolUse', toolAliases('read'))).toEqual([]);
  });
});

describe('hook runner', () => {
  it('passes JSON on stdin and maps exit 2 to a block with stderr', async () => {
    const cwd = tmp();
    const run = await runHook({ command: 'cat > stdin.json; echo "no rm" >&2; exit 2', timeoutMs: 5000, source: 't' }, { tool_name: 'bash' }, cwd);
    expect(JSON.parse(fs.readFileSync(path.join(cwd, 'stdin.json'), 'utf8'))).toEqual({ tool_name: 'bash' });
    expect(interpret('PreToolUse', run)).toEqual({ block: 'no rm' });
  });

  it('reads JSON decisions and context, ignores other exits and times out', async () => {
    const deny = { code: 0, stdout: JSON.stringify({ hookSpecificOutput: { permissionDecision: 'deny', permissionDecisionReason: 'nope', additionalContext: 'ctx' } }), stderr: '', timedOut: false };
    expect(interpret('PreToolUse', deny)).toEqual({ block: 'nope', context: 'ctx' });
    expect(interpret('PostToolUse', { ...deny, stdout: '{"decision":"block","reason":"fix lint"}' })).toEqual({ block: 'fix lint' });
    expect(interpret('SessionStart', { ...deny, stdout: 'plain context\n' })).toEqual({ context: 'plain context' });
    expect(interpret('PreToolUse', { ...deny, code: 2, stderr: '\u001b[31mno\u001b[0m rm\u0007' })).toEqual({ block: 'no rm' });
    expect(interpret('SessionStart', { ...deny, stdout: JSON.stringify({ hookSpecificOutput: { additionalContext: 'a\u001b]0;title\u0007b' } }) })).toEqual({ context: 'ab' });
    expect(interpret('PreToolUse', { ...deny, stdout: 'plain' })).toEqual({});
    expect(interpret('PreToolUse', { ...deny, code: 1 })).toEqual({});
    const slow = await runHook({ command: 'sleep 5', timeoutMs: 100, source: 't' }, {}, tmp());
    expect(slow.timedOut).toBe(true);
    expect(interpret('PreToolUse', { ...slow, code: 2 })).toEqual({});
  });

  it.skipIf(process.platform === 'win32')('kills the whole process group on timeout and returns promptly', async () => {
    const cwd = tmp();
    // The background grandchild keeps stdout open far past the test timeout: the run returns only if the group is killed.
    const run = await runHook({ command: 'cat > /dev/null; (sleep 60; echo done > survived.txt) & echo $! > child.pid; wait', timeoutMs: 300, source: 't' }, { a: 1 }, cwd);
    expect(run.timedOut).toBe(true);
    const pid = Number(fs.readFileSync(path.join(cwd, 'child.pid'), 'utf8'));
    await vi.waitFor(() => expect(alive(pid)).toBe(false), { timeout: 4000, interval: 50 });
    expect(fs.existsSync(path.join(cwd, 'survived.txt'))).toBe(false);
  }, 10_000);

  it.skipIf(process.platform === 'win32')('kills the process group on abort', async () => {
    const cwd = tmp();
    const controller = new AbortController();
    // Aborted once the hook is running; `sleep 60` outlives the test timeout, so returning at all proves the kill.
    const running = runHook({ command: 'echo $$ > shell.pid; sleep 60; echo done', timeoutMs: 30_000, source: 't' }, {}, cwd, {}, controller.signal);
    await vi.waitFor(() => expect(fs.existsSync(path.join(cwd, 'shell.pid'))).toBe(true), { timeout: 4000, interval: 20 });
    controller.abort();
    const run = await running;
    expect(run.timedOut).toBe(false);
    expect(run.stdout).toBe('');
  });

  it('caps context at 16 KB', () => {
    const capped = capContext('é'.repeat(20_000));
    expect(Buffer.byteLength(capped)).toBeLessThanOrEqual(HOOK_CONTEXT_MAX_BYTES);
    expect(capped).toMatch(/truncated\]$/);
    expect(capContext('short')).toBe('short');
  });
});

describe('hook payloads', () => {
  it('sends each file query in Claude Code shape and maps read path to file_path', () => {
    const cwd = tmp();
    const abs = (name: string) => path.join(cwd, name);
    const calls = hookCalls(
      'file',
      {
        queries: [
          { reasoning: 'r', type: 'edit', path: 'a.ts', edits: [{ oldText: 'x', newText: 'y' }] },
          { reasoning: 'r', type: 'edit', path: 'b.ts', edits: [{ oldText: '1', newText: '2' }, { oldText: '3', newText: '4' }] },
          { reasoning: 'r', type: 'write', path: 'c.ts', content: 'body' },
          { reasoning: 'r', type: 'delete', path: 'd.ts' },
        ],
      },
      cwd,
    );
    expect(calls).toEqual([
      { names: ['file', 'Edit'], tool_name: 'Edit', tool_input: { file_path: abs('a.ts'), old_string: 'x', new_string: 'y' } },
      { names: ['file', 'MultiEdit'], tool_name: 'MultiEdit', tool_input: { file_path: abs('b.ts'), edits: [{ old_string: '1', new_string: '2' }, { old_string: '3', new_string: '4' }] } },
      { names: ['file', 'Write'], tool_name: 'Write', tool_input: { file_path: abs('c.ts'), content: 'body' } },
      { names: ['file', 'Delete'], tool_name: 'Delete', tool_input: { file_path: abs('d.ts') } },
    ]);
    expect(hookCalls('read', { path: 'a.ts', offset: 3 }, cwd)).toEqual([{ names: ['read', 'Read'], tool_name: 'Read', tool_input: { file_path: abs('a.ts'), offset: 3 } }]);
    expect(hookCalls('bash', { command: 'ls' }, cwd)).toEqual([{ names: ['bash', 'Bash'], tool_name: 'Bash', tool_input: { command: 'ls' } }]);
    expect(hookCalls('web', { url: 'u' }, cwd)[0]).toMatchObject({ tool_name: 'web', tool_input: { url: 'u' } });
    expect(hookCalls('file', { queries: 'nope' }, cwd)).toEqual([]);
  });
});

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
    expect(((await fire('agent_before_settle', settle(), ctx)) as { continue: boolean }).continue).toBe(true);
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
    hooks.notification({ type: 'idle_prompt', message: 'idle' }, context(cwd) as never);
    hooks.notification({ type: 'permission_prompt', message: 'Save?', title: 'Pi: waiting for you' }, context(cwd) as never);
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
