import fs from 'node:fs';
import path from 'node:path';
import { withFileMutationQueue } from '@earendil-works/pi-coding-agent';
import { describe, expect, it, vi } from 'vitest';
import { Checkpoints } from '../src/files/checkpoint.js';
import { FileGuard } from '../src/files/guard.js';
import { registerFileTool } from '../src/files/tool.js';
import { fakeCtx, fakePermission, fakePi } from './fake-pi.js';
import { tmp } from './helpers.js';

function setup() {
  const cwd = fs.realpathSync(tmp());
  const file = path.join(cwd, 'note.txt');
  fs.writeFileSync(file, 'original');
  const guard = new FileGuard();
  guard.record(file);
  const checkpoints = new Checkpoints();
  checkpoints.open(tmp());
  checkpoints.beginTurn();
  const fake = fakePi();
  registerFileTool(fake.pi, guard, fakePermission(), checkpoints);
  return { cwd, file, guard, checkpoints, tool: fake.tools.get('file'), ctx: fakeCtx({ cwd }) };
}

describe('file cancellation around filesystem mutations', () => {
  it('does not delete a file when aborted while waiting for its mutation queue', async () => {
    const { file, checkpoints, tool, ctx } = setup();
    const controller = new AbortController();
    let release: () => void = () => undefined;
    const held = withFileMutationQueue(file, () => new Promise<void>((resolve) => { release = resolve; }));
    const capture = vi.spyOn(checkpoints, 'capture');
    const running = tool.execute('cancel-delete', { queries: [{ type: 'delete', path: file, reasoning: 'remove it' }] }, controller.signal, undefined, ctx);
    await vi.waitFor(() => expect(capture).toHaveBeenCalledOnce());
    controller.abort();
    release();
    await held;
    await expect(running).rejects.toThrow(/abort/i);
    expect(fs.readFileSync(file, 'utf8')).toBe('original');
    expect(checkpoints.list()).toEqual([]);
  });

  it.each(['write', 'edit'] as const)('keeps the checkpoint if %s is aborted after its atomic replacement', async (type) => {
    const { file, guard, checkpoints, tool, ctx } = setup();
    const controller = new AbortController();
    const rename = fs.promises.rename.bind(fs.promises);
    const changed = vi.spyOn(fs.promises, 'rename').mockImplementation(async (from, to) => {
      await rename(from, to);
      if (to === file) controller.abort();
    });
    try {
      const query = { type, path: file, reasoning: 'replace it', ...(type === 'write' ? { content: 'changed' } : { edits: [{ oldText: 'original', newText: 'changed' }] }) };
      await expect(tool.execute('cancel-write', { queries: [query] }, controller.signal, undefined, ctx)).rejects.toThrow(/abort[\s\S]*The file was changed/i);
      expect(fs.readFileSync(file, 'utf8')).toBe('changed');
      expect(checkpoints.list()).toHaveLength(1);
      expect(guard.check(file, 'edit', file)).toBeUndefined();
      changed.mockRestore();
      expect((await checkpoints.rewind(checkpoints.turns())).restored).toEqual([file]);
      expect(fs.readFileSync(file, 'utf8')).toBe('original');
    } finally {
      changed.mockRestore();
    }
  });
});
