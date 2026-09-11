import { describe, expect, it } from 'vitest';
import { entry, hasDriftedCommand, isExactHookEntry, specsFor } from '../src/hooks-install-health.js';

describe('hook profile tool matchers', () => {
  const params = { globalMode: false, projectDir: '/work/repo', hookDir: '/work/hooks' };

  it('keeps coordination on lifecycle boundaries while guard stays mutation-selective', () => {
    const coordination = specsFor('codex', { ...params, profile: 'coordination' });
    expect(coordination.find(({ event }) => event === 'PreToolUse')).toBeUndefined();
    expect(coordination.find(({ event }) => event === 'PostToolUse')).toBeUndefined();

    const guard = specsFor('codex', { ...params, profile: 'guard' });
    expect(guard.find(({ event }) => event === 'PreToolUse')?.matcher).toContain('apply_patch');
    expect(guard.find(({ event }) => event === 'PostToolUse')?.matcher).toContain('Write');
  });

  it('uses the Codex SessionEnd limit for generation and drift detection', () => {
    const spec = specsFor('codex', { ...params, profile: 'full' }).find(({ event }) => event === 'SessionEnd')!;
    const current = entry('codex', spec);
    expect(current.hooks?.[0]?.timeout).toBe(3);
    expect(isExactHookEntry('codex', current, spec)).toBe(true);
    const old = { ...current, hooks: current.hooks!.map(hook => ({ ...hook, timeout: 20 })) };
    expect(isExactHookEntry('codex', old, spec)).toBe(false);
    expect(hasDriftedCommand([old], 'codex', spec)).toBe(true);
    const other = specsFor('codex', { ...params, profile: 'full' }).find(({ event }) => event === 'PostToolUse')!;
    expect(entry('codex', other).hooks?.[0]?.timeout).toBe(20);
  });
});
