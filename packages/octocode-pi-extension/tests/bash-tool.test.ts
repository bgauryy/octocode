import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { test } from 'vitest';
import { extractBashWriteTargets } from '../src/tools/bash-tool.js';

test('extractBashWriteTargets finds redirects and tee', () => {
  const cwd = '/tmp/work';
  assert.deepEqual(extractBashWriteTargets('echo hi > out.txt', cwd), [
    path.join(cwd, 'out.txt'),
  ]);
  assert.deepEqual(extractBashWriteTargets('echo hi >> /tmp/abs.log', cwd), [
    '/tmp/abs.log',
  ]);
  assert.ok(
    extractBashWriteTargets('printf x | tee nested/a.txt', cwd).includes(
      path.join(cwd, 'nested/a.txt'),
    ),
  );
});

test('extractBashWriteTargets finds cp/mv destinations', () => {
  const cwd = '/tmp/work';
  const targets = extractBashWriteTargets('cp a.ts b.ts', cwd);
  assert.deepEqual(targets, [path.join(cwd, 'b.ts')]);
});

test('bash override blocks writes outside allowed roots', async () => {
  const { default: extension } = await import('../src/index.js');
  const tools = new Map<
    string,
    {
      name: string;
      label?: string;
      execute: (
        id: string,
        params: Record<string, unknown>,
        sig?: AbortSignal,
        upd?: unknown,
        ctx?: { cwd?: string },
      ) => Promise<{ content: Array<{ type: string; text: string }>; isError?: boolean }>;
    }
  >();
  const active = ['bash', 'edit', 'write'];
  await extension({
    on: () => undefined,
    sendUserMessage: () => undefined,
    registerTool: (def: { name: string }) => {
      tools.set(def.name, def as (typeof tools extends Map<string, infer V> ? V : never));
    },
    getActiveTools: () => [...active],
    setActiveTools: (names: string[]) => {
      active.splice(0, active.length, ...names);
    },
  });
  const bash = tools.get('bash')!;
  assert.equal(bash.label, 'bash (Octocode)');
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-bash-'));
  try {
    await assert.rejects(
      () =>
        bash.execute(
          '1',
          { command: `echo pwned > /usr/octocode-bash-block-${process.pid}.txt` },
          undefined,
          undefined,
          { cwd: tmp },
        ),
      /bash write blocked|outside the allowed roots/,
    );
    const ok = await bash.execute(
      '2',
      { command: 'echo hello > ok.txt && cat ok.txt' },
      undefined,
      undefined,
      { cwd: tmp },
    );
    assert.match(ok.content[0]!.text, /hello/);
    assert.equal(fs.readFileSync(path.join(tmp, 'ok.txt'), 'utf8').trim(), 'hello');
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});
