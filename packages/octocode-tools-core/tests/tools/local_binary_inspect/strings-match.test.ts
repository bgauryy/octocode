import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import { inspectBinary } from '../../../src/tools/local_binary_inspect/binaryInspector.js';

const tempDirs: string[] = [];

async function tempFile(name: string, content: string): Promise<string> {
  const dir = await mkdtemp(
    join(process.cwd(), '.tmp-octocode-binary-strings-')
  );
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

describe('localBinaryInspect strings matchString', () => {
  it('filters extracted strings with the same matchString surface as text modes', async () => {
    const file = await tempFile(
      'sample.bin',
      'alpha_secret_token\nplain_public_value\nbeta_secret_token\n'
    );

    const result = await inspectBinary({
      path: file,
      mode: 'strings',
      minLength: 4,
      matchString: 'secret',
      matchStringContextLines: 0,
    });

    expect(result.status).toBe('success');
    expect(result.mode).toBe('strings');
    expect(result.content).toContain('alpha_secret_token');
    expect(result.content).toContain('beta_secret_token');
    expect(result.content).not.toContain('plain_public_value');
    expect(result.localPath).toContain('sample.bin.strings.txt');
  });

  it('returns an explicit error when no extracted string matches', async () => {
    const file = await tempFile('sample.bin', 'plain_public_value\n');

    const result = await inspectBinary({
      path: file,
      mode: 'strings',
      minLength: 4,
      matchString: 'secret',
      matchStringContextLines: 0,
    });

    expect(result.status).toBe('error');
    expect(result.error).toContain('No lines match "secret"');
  });
});
