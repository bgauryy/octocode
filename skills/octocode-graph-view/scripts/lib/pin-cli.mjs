import { createHash } from 'node:crypto';
import { constants, createReadStream } from 'node:fs';
import { chmod, copyFile, link, lstat, mkdir, mkdtemp, rm } from 'node:fs/promises';
import { tmpdir, userInfo } from 'node:os';
import { extname, join } from 'node:path';

async function digest(file) {
  const hash = createHash('sha256');
  for await (const bytes of createReadStream(file)) hash.update(bytes);
  return hash.digest('hex');
}

// An immutable image per binary content keeps the native server warm between runs.
// Publish with a hard link from a private copy, so concurrent builders never replace a live image.
export async function freezeExecutable(source, cache = join(tmpdir(), `octocode-graph-cli-${userInfo().uid}`)) {
  await mkdir(cache, { recursive: true, mode: 0o700 });
  const dir = await lstat(cache);
  if (!dir.isDirectory() || (process.getuid && (dir.uid !== process.getuid() || (dir.mode & 0o077)))) {
    throw new Error(`The graph executable cache must be a private directory: ${cache}`);
  }
  const key = await digest(source);
  const image = join(cache, key + extname(source));
  try {
    if (!(await lstat(image)).isFile() || await digest(image) !== key) throw new Error(`Invalid frozen executable: ${image}`);
    return image;
  } catch (error) {
    if (error.code !== 'ENOENT') throw error;
  }
  const staging = await mkdtemp(join(cache, 'copy-'));
  const copy = join(staging, 'image');
  try {
    await copyFile(source, copy, constants.COPYFILE_FICLONE);
    if (await digest(copy) !== key) throw new Error('The Octocode executable changed while it was being pinned. Rerun the build.');
    await chmod(copy, 0o500);
    try { await link(copy, image); } catch (error) { if (error.code !== 'EEXIST') throw error; }
    return image;
  } finally {
    await rm(staging, { recursive: true, force: true });
  }
}
