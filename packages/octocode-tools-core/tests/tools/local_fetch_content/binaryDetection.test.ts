import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';

import { isLikelyBinaryFile } from '../../../src/tools/local_fetch_content/fetchContent/validation.js';

const dirs: string[] = [];

afterEach(async () => {
  await Promise.all(dirs.splice(0).map(dir => rm(dir, { recursive: true })));
});

describe('isLikelyBinaryFile', () => {
  it('accepts valid UTF-8 when the fixed-size sample ends inside a multibyte character', async () => {
    const dir = await mkdtemp(join(tmpdir(), 'octocode-utf8-boundary-'));
    dirs.push(dir);
    const file = join(dir, 'boundary.ts');
    await writeFile(file, `${'a'.repeat(8191)}─tail`, 'utf8');

    await expect(isLikelyBinaryFile(file)).resolves.toBe(false);
  });

  it('still rejects a file containing NUL bytes', async () => {
    const dir = await mkdtemp(join(tmpdir(), 'octocode-binary-'));
    dirs.push(dir);
    const file = join(dir, 'binary.bin');
    await writeFile(file, Buffer.from([0x61, 0x00, 0x62]));

    await expect(isLikelyBinaryFile(file)).resolves.toBe(true);
  });
});
