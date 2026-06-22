import * as fsPromises from 'fs/promises';
import * as os from 'os';
import * as path from 'path';
import { afterEach, describe, expect, it } from 'vitest';

import { readJsonFile } from '../src/jsonUtils';

const temporaryDirectories: string[] = [];

afterEach(async () => {
  await Promise.all(
    temporaryDirectories
      .splice(0)
      .map(dirPath => fsPromises.rm(dirPath, { recursive: true, force: true }))
  );
});

async function makeTempDir(): Promise<string> {
  const dirPath = await fsPromises.mkdtemp(
    path.join(os.tmpdir(), 'octocode-vscode-')
  );
  temporaryDirectories.push(dirPath);
  return dirPath;
}

describe('readJsonFile', () => {
  it('returns parsed JSON for valid files', async () => {
    const dirPath = await makeTempDir();
    const filePath = path.join(dirPath, 'config.json');
    await fsPromises.writeFile(
      filePath,
      JSON.stringify({ enabled: true }),
      'utf8'
    );

    await expect(readJsonFile<{ enabled: boolean }>(filePath)).resolves.toEqual(
      {
        enabled: true,
      }
    );
  });

  it('returns null for missing files', async () => {
    const dirPath = await makeTempDir();
    const filePath = path.join(dirPath, 'missing.json');

    await expect(readJsonFile(filePath)).resolves.toBeNull();
  });

  it('returns null for empty files', async () => {
    const dirPath = await makeTempDir();
    const filePath = path.join(dirPath, 'empty.json');
    await fsPromises.writeFile(filePath, '   ', 'utf8');

    await expect(readJsonFile(filePath)).resolves.toBeNull();
  });

  it('returns null for invalid JSON', async () => {
    const dirPath = await makeTempDir();
    const filePath = path.join(dirPath, 'broken.json');
    await fsPromises.writeFile(filePath, '{ broken', 'utf8');

    await expect(readJsonFile(filePath)).resolves.toBeNull();
  });
});
