import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import { inspectBinary } from '../../../src/tools/local_binary_inspect/binaryInspector.js';

const tempDirs: string[] = [];

async function tempFile(name: string, content: Buffer | string): Promise<string> {
  const dir = await mkdtemp(join(process.cwd(), '.tmp-octocode-binary-inspect-'));
  tempDirs.push(dir);
  const file = join(dir, name);
  await writeFile(file, content);
  return file;
}

afterEach(async () => {
  await Promise.all(
    tempDirs.splice(0).map(dir => rm(dir, { recursive: true, force: true }))
  );
});

describe('localBinaryInspect inspect + strings next', () => {
  it('inspects a small Mach-O-like buffer via native engine path', async () => {
    // Minimal gzip magic — identify_format should still return a format label.
    const file = await tempFile(
      'sample.gz',
      Buffer.from([0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03])
    );

    const result = await inspectBinary({
      path: file,
      mode: 'inspect',
    });

    expect(result.status).toBe('success');
    expect(result.mode).toBe('inspect');
    expect(result.format).toBeTruthy();
    expect(typeof result.path).toBe('string');
  });

  it('emits next.continueChars when strings preview is truncated', async () => {
    const payload = Array.from({ length: 400 }, (_, i) =>
      `token_value_${i}_abcdefghijklmnopqrstuvwxyz`
    ).join('\n');
    const file = await tempFile('big.bin', payload);

    const result = await inspectBinary({
      path: file,
      mode: 'strings',
      minLength: 8,
      charLength: 500,
      charOffset: 0,
    });

    expect(result.status).toBe('success');
    expect(result.mode).toBe('strings');
    expect(result.isPartial).toBe(true);
    expect(result.next?.continueChars?.tool).toBe('localBinaryInspect');
    expect(result.next?.continueChars?.query).toMatchObject({
      mode: 'strings',
      charOffset: expect.any(Number),
    });
  });
});
