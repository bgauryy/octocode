import { describe, expect, it } from 'vitest';
import { dialogsWaiting, exclusive, forgetCheck, onDialogsChange, recordCheck, takeTiming, withDialog } from '../src/shared/locks.js';

const tick = () => new Promise((resolve) => setTimeout(resolve, 0));

describe('exclusive', () => {
  it('runs calls one at a time in call order, past failures, leaving aborts to the tool', async () => {
    const log: string[] = [];
    let release!: () => void;
    const gate = new Promise<void>((resolve) => (release = resolve));
    const run = exclusive(async (id: string, _params: unknown, signal: AbortSignal | undefined) => {
      if (signal?.aborted) return `${id} cancelled`;
      log.push(`start ${id}`);
      if (id === 'a') await gate;
      if (id === 'b') throw new Error('b failed');
      log.push(`end ${id}`);
      return id;
    });
    const aborted = new AbortController();
    const a = run('a', {}, undefined);
    const b = run('b', {}, undefined);
    const c = run('c', {}, aborted.signal);
    const d = run('d', {}, undefined);
    aborted.abort();
    await Promise.resolve();
    expect(log).toEqual(['start a']);
    release();
    await expect(a).resolves.toBe('a');
    await expect(b).rejects.toThrow('b failed');
    await expect(c).resolves.toBe('c cancelled');
    await expect(d).resolves.toBe('d');
    expect(log).toEqual(['start a', 'end a', 'start b', 'start d', 'end d']);
  });

  it('lets a holder re-enter its own lock instead of deadlocking', async () => {
    let inner!: (id: string) => Promise<string>;
    const run = exclusive(async (id: string): Promise<string> => (id === 'outer' ? `outer(${await inner('inner')})` : id));
    inner = run;
    await expect(run('outer')).resolves.toBe('outer(inner)');
  });
});

describe('withDialog', () => {
  it('shows one interaction at a time, in call order, and counts the waiting ones', async () => {
    const log: string[] = [];
    const counts: number[] = [];
    const unsubscribe = onDialogsChange(() => counts.push(dialogsWaiting()));
    let close!: () => void;
    const first = withDialog(async () => {
      log.push('first open');
      await new Promise<void>((resolve) => (close = resolve));
      log.push('first close');
    });
    const second = withDialog(async () => {
      log.push('second open');
    });
    await tick();
    expect(log).toEqual(['first open']);
    expect(dialogsWaiting()).toBe(1);
    close();
    await Promise.all([first, second]);
    expect(log).toEqual(['first open', 'first close', 'second open']);
    expect(dialogsWaiting()).toBe(0);
    expect(counts).toContain(1);
    unsubscribe();
  });

  it('runs a nested interaction at once and an aborted one without waiting, but keeps later ones in line', async () => {
    const log: string[] = [];
    let close!: () => void;
    const holder = withDialog(async () => {
      log.push('holder');
      await withDialog(async () => log.push('nested'));
      await new Promise<void>((resolve) => (close = resolve));
      log.push('holder done');
    });
    await tick();
    const abort = new AbortController();
    const aborted = withDialog(async () => log.push('aborted ran'), abort.signal);
    const later = withDialog(async () => log.push('later'));
    abort.abort();
    await tick();
    expect(log).toEqual(['holder', 'nested', 'aborted ran']);
    close();
    await Promise.all([holder, aborted, later]);
    expect(log).toEqual(['holder', 'nested', 'aborted ran', 'holder done', 'later']);
    await expect(withDialog(async () => 'pre-aborted', AbortSignal.abort())).resolves.toBe('pre-aborted');
  });
});

describe('check timing', () => {
  it('records, takes once, forgets and stays bounded', () => {
    recordCheck('a', 300, 1_000);
    expect(takeTiming('a', 1_500)).toEqual({ checkMs: 300, queuedMs: 500 });
    expect(takeTiming('a', 1_500)).toEqual({});
    recordCheck('b', 10, 1_000);
    expect(takeTiming('b', 1_050)).toEqual({});
    recordCheck('c', 300);
    forgetCheck('c');
    expect(takeTiming('c')).toEqual({});
    for (let index = 0; index < 600; index += 1) recordCheck(`bulk-${index}`, 200, 0);
    expect(takeTiming('bulk-0', 0)).toEqual({});
    expect(takeTiming('bulk-599', 0)).toEqual({ checkMs: 200 });
  });
});
