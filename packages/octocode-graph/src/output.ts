import { randomUUID } from 'node:crypto';
import { link, rm, writeFile } from 'node:fs/promises';

/** Publish a complete output beside its scratch file without overwriting a concurrent winner. */
export async function writeNewFile(
  path: string,
  contents: string | AsyncIterable<string>
) {
  const temporary = `${path}.${randomUUID()}.tmp`;
  try {
    await writeFile(temporary, contents, { flag: 'wx' });
    await link(temporary, path);
  } finally {
    await rm(temporary, { force: true });
  }
}
