import fs from 'node:fs';
import path from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { Checkpoints } from '../src/files/checkpoint.js';
import { inheritCheckpoints } from '../src/files/checkpoint-store.js';
import { outputDir, privateDir } from '../src/shared/home.js';
import { saveFullOutput } from '../src/shared/spill.js';
import { tmp } from './helpers.js';

const modeOf = (file: string) => fs.statSync(file).mode & 0o777;

// POSIX permission bits only; Windows has no owner-only mode to check.
describe.skipIf(process.platform === 'win32')('runtime state is owner-only', () => {
  let saved: string | undefined;
  let home: string;
  beforeEach(() => {
    saved = process.env['OCTOCODE_HOME'];
    home = path.join(tmp(), 'home');
    process.env['OCTOCODE_HOME'] = home;
  });
  afterEach(() => {
    if (saved === undefined) delete process.env['OCTOCODE_HOME'];
    else process.env['OCTOCODE_HOME'] = saved;
  });

  it('creates output folders 0700 and tightens one that already exists', () => {
    expect(modeOf(outputDir('pi-x'))).toBe(0o700);
    expect(modeOf(home)).toBe(0o700);
    const loose = path.join(home, 'pi-loose');
    fs.mkdirSync(loose, { mode: 0o755 });
    fs.chmodSync(loose, 0o755);
    expect(modeOf(outputDir('pi-loose'))).toBe(0o700);
    expect(modeOf(privateDir(path.join(tmp(), 'a', 'b')))).toBe(0o700);
  });

  it('writes spilled output 0600', () => {
    const file = saveFullOutput('secret', 'x')!;
    expect(modeOf(file)).toBe(0o600);
    expect(modeOf(path.dirname(file))).toBe(0o700);
  });

  it('keeps checkpoint folders 0700 and blobs and journals 0600, inherited ones too', async () => {
    const cwd = tmp();
    const file = path.join(cwd, 'a.txt');
    fs.writeFileSync(file, 'before');
    const store = path.join(tmp(), 'cp');
    const checkpoints = new Checkpoints();
    checkpoints.open(store);
    checkpoints.beginTurn();
    const capture = await checkpoints.capture(file, () => 'u1');
    fs.writeFileSync(file, 'after');
    await checkpoints.settle(file, capture, true);
    const blobs = path.join(store, 'blobs');
    expect(modeOf(store)).toBe(0o700);
    expect(modeOf(blobs)).toBe(0o700);
    for (const blob of fs.readdirSync(blobs)) expect(modeOf(path.join(blobs, blob))).toBe(0o600);
    expect(modeOf(path.join(store, 'journal.jsonl'))).toBe(0o600);

    const fork = path.join(tmp(), 'fork');
    expect(await inheritCheckpoints(store, fork, ['u1'])).toBe(1);
    expect(modeOf(fork)).toBe(0o700);
    expect(modeOf(path.join(fork, 'blobs'))).toBe(0o700);
    expect(modeOf(path.join(fork, 'journal.jsonl'))).toBe(0o600);
  });
});
