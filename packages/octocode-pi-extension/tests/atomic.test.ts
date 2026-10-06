import fs from 'node:fs';
import path from 'node:path';
import { describe, expect, it } from 'vitest';
import { atomicWriteFile, sha256, sha256File } from '../src/shared/atomic.js';
import { FileGuard, registerFileTool } from '../src/files/tool.js';
import { tmp } from './helpers.js';

const noTemp = (dir: string) => fs.readdirSync(dir).filter((name) => name.endsWith('.tmp'));

function fileTool(guard = new FileGuard()) {
  let tool: { execute: (...args: unknown[]) => Promise<{ content: Array<{ text: string }> }> } | undefined;
  registerFileTool({ registerTool: (definition: never) => (tool = definition), on: () => undefined } as never, guard);
  return { guard, run: (cwd: string, queries: unknown[]) => tool!.execute('t', { queries }, undefined, undefined, { cwd, hasUI: false }) };
}

describe('atomic writes', () => {
  it('replaces the file, keeps its mode and leaves no temp file', async () => {
    const dir = tmp();
    const file = path.join(dir, 'a.sh');
    fs.writeFileSync(file, 'old');
    fs.chmodSync(file, 0o754);
    await atomicWriteFile(file, 'new', sha256('old'));
    expect(fs.readFileSync(file, 'utf8')).toBe('new');
    expect(fs.statSync(file).mode & 0o777).toBe(0o754);
    expect(noTemp(dir)).toEqual([]);
  });

  it('refuses when the content changed since it was read, midway through the write, leaving the target intact', async () => {
    const dir = tmp();
    const file = path.join(dir, 'a.txt');
    fs.writeFileSync(file, 'theirs');
    await expect(atomicWriteFile(file, 'mine', sha256('what I read'))).rejects.toThrow(/changed on disk/);
    expect(fs.readFileSync(file, 'utf8')).toBe('theirs');
    expect(noTemp(dir)).toEqual([]);
    // null: the file must still be absent.
    await expect(atomicWriteFile(file, 'mine', null)).rejects.toThrow(/changed on disk/);
    await atomicWriteFile(path.join(dir, 'new.txt'), 'fresh', null);
    expect(fs.readFileSync(path.join(dir, 'new.txt'), 'utf8')).toBe('fresh');
  });

  it('writes through a symlink instead of replacing it', async () => {
    const dir = tmp();
    const real = path.join(dir, 'real.txt');
    const link = path.join(dir, 'link.txt');
    fs.writeFileSync(real, 'one');
    fs.symlinkSync(real, link);
    await atomicWriteFile(link, 'two');
    expect(fs.lstatSync(link).isSymbolicLink()).toBe(true);
    expect(fs.readFileSync(real, 'utf8')).toBe('two');
  });

  it('hashes files and reports missing ones', () => {
    const dir = tmp();
    fs.writeFileSync(path.join(dir, 'a'), 'x');
    expect(sha256File(path.join(dir, 'a'))).toBe(sha256('x'));
    expect(sha256File(path.join(dir, 'missing'))).toBeUndefined();
    expect(sha256File(dir)).toBeUndefined();
  });
});

describe('file tool content precondition', () => {
  it('refuses a same-size rewrite that kept the mtime', async () => {
    const cwd = tmp();
    const file = path.join(cwd, 'a.txt');
    fs.writeFileSync(file, 'aaaa');
    const { guard, run } = fileTool();
    guard.record(file);
    const { mtime } = fs.statSync(file);
    fs.writeFileSync(file, 'bbbb');
    fs.utimesSync(file, mtime, mtime);
    expect(guard.check(file, 'write', 'a.txt')).toMatch(/changed on disk/);
    await expect(run(cwd, [{ reasoning: 'r', type: 'write', path: 'a.txt', content: 'cccc' }])).rejects.toThrow(/changed on disk/);
    expect(fs.readFileSync(file, 'utf8')).toBe('bbbb');
  });

  it('edits atomically and keeps the file mode', async () => {
    const cwd = tmp();
    const file = path.join(cwd, 'run.sh');
    fs.writeFileSync(file, 'echo one\n');
    fs.chmodSync(file, 0o755);
    const { run } = fileTool();
    const result = await run(cwd, [{ reasoning: 'r', type: 'edit', path: 'run.sh', edits: [{ oldText: 'one', newText: 'two' }] }]);
    expect(result.content[0]!.text).toContain('1. OK edit run.sh');
    expect(fs.readFileSync(file, 'utf8')).toBe('echo two\n');
    expect(fs.statSync(file).mode & 0o777).toBe(0o755);
    expect(noTemp(cwd)).toEqual([]);
  });

  it('writes new files and replaces read ones', async () => {
    const cwd = tmp();
    const { guard, run } = fileTool();
    await run(cwd, [{ reasoning: 'r', type: 'write', path: 'sub/new.txt', content: 'hello' }]);
    const file = path.join(cwd, 'sub', 'new.txt');
    expect(fs.readFileSync(file, 'utf8')).toBe('hello');
    guard.record(file);
    await run(cwd, [{ reasoning: 'r', type: 'write', path: 'sub/new.txt', content: 'bye' }]);
    expect(fs.readFileSync(file, 'utf8')).toBe('bye');
  });
});
