import { createHash } from 'node:crypto';
import { constants, createReadStream } from 'node:fs';
import {
  chmod,
  copyFile,
  link,
  lstat,
  mkdir,
  mkdtemp,
  readFile,
  rename,
  rm,
  stat,
  writeFile,
} from 'node:fs/promises';
import { tmpdir, userInfo } from 'node:os';
import { extname, join } from 'node:path';

async function digest(file: string) {
  const hash = createHash('sha256');
  for await (const bytes of createReadStream(file)) hash.update(bytes);
  return hash.digest('hex');
}

// The source's digest, hashed once per file identity. Device, inode, size and both times name one content: a
// rebuild or reinstall writes a new inode or new times, and the change time follows any edit and cannot be set
// back. A run then pins in milliseconds instead of hashing the executable (hundreds of MB) again.
async function sourceDigest(source: string, cache: string) {
  const s = await stat(source, { bigint: true });
  const stamp = join(
    cache,
    'digests',
    `${s.dev}-${s.ino}-${s.size}-${s.mtimeNs}-${s.ctimeNs}`
  );
  const known = await readFile(stamp, 'utf8').catch(() => null);
  if (known && /^[0-9a-f]{64}$/.test(known)) return known;
  const key = await digest(source);
  await mkdir(join(cache, 'digests'), { recursive: true, mode: 0o700 });
  const temporary = `${stamp}.${process.pid}.tmp`;
  await writeFile(temporary, key);
  await rename(temporary, stamp);
  return key;
}

// A filesystem error with this code (ENOENT, EEXIST); any other thrown value is not one.
const isCode = (error: unknown, code: string) =>
  error instanceof Error && 'code' in error && error.code === code;

// An immutable image per binary content keeps the native server warm between runs.
// Publish with a hard link from a private copy, so concurrent builders never replace a live image.
export async function freezeExecutable(
  source: string,
  cache = join(tmpdir(), `octocode-graph-cli-${userInfo().uid}`)
) {
  await mkdir(cache, { recursive: true, mode: 0o700 });
  const dir = await lstat(cache);
  if (
    !dir.isDirectory() ||
    (process.getuid && (dir.uid !== process.getuid() || dir.mode & 0o077))
  ) {
    throw new Error(
      `The graph executable cache must be a private directory: ${cache}`
    );
  }
  const key = await sourceDigest(source, cache);
  const image = join(cache, key + extname(source));
  // An image is published only after its copy hashed to its name, and stays read-only in this private
  // directory: a regular, unwritable file of the source's size is that image.
  try {
    const [found, original] = await Promise.all([lstat(image), stat(source)]);
    if (!found.isFile() || found.mode & 0o222 || found.size !== original.size)
      throw new Error(`Invalid frozen executable: ${image}`);
    return image;
  } catch (error) {
    if (!isCode(error, 'ENOENT')) throw error;
  }
  const staging = await mkdtemp(join(cache, 'copy-'));
  const copy = join(staging, 'image');
  try {
    await copyFile(source, copy, constants.COPYFILE_FICLONE);
    if ((await digest(copy)) !== key)
      throw new Error(
        'The Octocode executable changed while it was being pinned. Rerun the build.'
      );
    await chmod(copy, 0o500);
    try {
      await link(copy, image);
    } catch (error) {
      if (!isCode(error, 'EEXIST')) throw error;
    }
    return image;
  } finally {
    await rm(staging, { recursive: true, force: true });
  }
}
