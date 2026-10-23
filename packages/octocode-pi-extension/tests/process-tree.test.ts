import { spawn } from 'node:child_process';
import { describe, expect, it, vi } from 'vitest';
import { killTree, processAlive, stopTree } from '../src/shared/process.js';

/** A child that ignores SIGTERM and has started a detached grandchild in its own process group; resolves with both pids. */
const treeWithDetachedGrandchild = async () => {
  const script = `process.on('SIGTERM', () => {}); const g = require('node:child_process').spawn(process.execPath, ['-e', 'setInterval(() => {}, 1000)'], { detached: true, stdio: 'ignore' }); g.unref(); console.log(g.pid); setInterval(() => {}, 1000);`;
  const child = spawn(process.execPath, ['-e', script], { detached: true, stdio: ['ignore', 'pipe', 'ignore'] });
  const grandchild = await new Promise<number>((resolve) => child.stdout.once('data', (chunk: Buffer) => resolve(Number(String(chunk).trim()))));
  return { child, grandchild };
};

describe('process', () => {
  it('checks liveness', () => {
    expect(processAlive(process.pid)).toBe(true);
    expect(processAlive(0)).toBe(false);
    expect(processAlive(-1)).toBe(false);
    expect(processAlive('1')).toBe(false);
    expect(processAlive(2 ** 22 + 12345)).toBe(false);
  });

  it.skipIf(process.platform === 'win32')('kills a detached process tree', async () => {
    const child = spawn('sh', ['-c', 'sleep 30 & wait'], { detached: true, stdio: 'ignore' });
    const exited = new Promise((resolve) => child.on('exit', resolve));
    killTree(child.pid, 100);
    await exited;
    expect(processAlive(child.pid)).toBe(false);
    killTree(undefined);
  });

  it.skipIf(process.platform === 'win32')('stopTree also stops descendants that left the process group', async () => {
    const { child, grandchild } = await treeWithDetachedGrandchild();
    try {
      expect(processAlive(grandchild)).toBe(true);
      await stopTree(child.pid, 200);
      await vi.waitFor(() => expect(processAlive(grandchild)).toBe(false), { timeout: 2_000 });
    } finally {
      try { process.kill(grandchild, 'SIGKILL'); } catch { /* gone */ }
    }
  });

  it.skipIf(process.platform === 'win32')('killTree also kills descendants that left the process group once the grace runs out', async () => {
    const { child, grandchild } = await treeWithDetachedGrandchild();
    try {
      killTree(child.pid, 200);
      await vi.waitFor(() => expect(processAlive(grandchild)).toBe(false), { timeout: 2_000 });
    } finally {
      try { process.kill(grandchild, 'SIGKILL'); } catch { /* gone */ }
    }
  });

  it.skipIf(process.platform === 'win32')('stops a tree once and sends no SIGKILL after it closed', async () => {
    const child = spawn('sh', ['-c', 'sleep 30'], { detached: true, stdio: 'ignore' });
    const closed = new Promise((resolve) => child.on('close', resolve));
    const kill = vi.spyOn(process, 'kill');
    try {
      killTree(child.pid, 150);
      killTree(child.pid, 150);
      await closed;
      await new Promise((resolve) => setTimeout(resolve, 250));
      const signals = kill.mock.calls.filter(([pid]) => pid === -child.pid!).map(([, signal]) => signal);
      expect(signals.filter((signal) => signal === 'SIGTERM')).toHaveLength(1);
      expect(signals).not.toContain('SIGKILL');
    } finally {
      kill.mockRestore();
    }
  });
});
