import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { describe, expect, it, vi } from 'vitest';
import { capContext, HOOK_CONTEXT_MAX_BYTES, interpret, runHook } from '../src/hooks/runner.js';
import { tmp } from './helpers.js';
import { useHooksHome } from './hooks-harness.js';

useHooksHome();

/** Whether a process is still running. */
function alive(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

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
    expect(interpret('PreToolUse', { ...deny, code: 2, stderr: '\u001b[31mno\u001b[0m rm\u0007' })).toEqual({ block: '\\u{1b}[31mno\\u{1b}[0m rm\\u{7}' });
    expect(interpret('SessionStart', { ...deny, stdout: JSON.stringify({ hookSpecificOutput: { additionalContext: 'a\u001b]0;title\u0007b' } }) })).toEqual({ context: 'a\\u{1b}]0;title\\u{7}b' });
    const ask = (decision: string, reason?: string) => ({ ...deny, stdout: JSON.stringify({ hookSpecificOutput: { permissionDecision: decision, ...(reason ? { permissionDecisionReason: reason } : {}) } }) });
    expect(interpret('PreToolUse', ask('ask', 'prod \u001b[2Jdeploy'))).toEqual({ ask: 'prod \\u{1b}[2Jdeploy' });
    expect(interpret('PreToolUse', ask('ask'))).toEqual({ ask: expect.stringMatching(/hook asks/) });
    expect(interpret('PostToolUse', ask('ask', 'x'))).toEqual({});
    // A hook's allow is not honoured: it never skips an ask.
    expect(interpret('PreToolUse', ask('allow', 'fine'))).toEqual({});
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

  it('keeps context within 16 KB by saving the whole of a longer one to a file it names', () => {
    const text = Array.from({ length: 2_000 }, (_, i) => `line ${i} ${'é'.repeat(10)}`).join('\n');
    const capped = capContext(text);
    expect(Buffer.byteLength(capped)).toBeLessThanOrEqual(HOOK_CONTEXT_MAX_BYTES);
    expect(capped).toMatch(/^line 0 /);
    expect(capped).toMatch(/line 1999 é+\n\n\[Output trimmed: showing \d+ of 2000 lines .*Full output: \S+hook-\S+\.txt;/);
    const shown = /Full output: (\S+);/.exec(capped)![1];
    expect(fs.readFileSync(shown.startsWith('~') ? path.join(os.homedir(), shown.slice(2)) : shown, 'utf8')).toBe(text);
    expect(capContext('short')).toBe('short');
  });

  it('says how much output past the read limit was not kept', () => {
    const run = { code: 0, stdout: 'hello', stderr: '', timedOut: false, dropped: { stdout: 2048, stderr: 0 } };
    expect(interpret('SessionStart', run).context).toBe('hello\n[… 2.0KB more output not kept: a hook\'s output is read up to 64.0KB]');
    expect(interpret('PreToolUse', { ...run, code: 2, stderr: 'no', dropped: { stdout: 0, stderr: 10 } }).block).toMatch(/^no\n\[… 10B more output not kept/);
  });
});
