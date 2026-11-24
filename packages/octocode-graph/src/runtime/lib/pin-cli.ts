import { createHash } from 'node:crypto';
import { constants, createReadStream } from 'node:fs';
import {
  chmod,
  copyFile,
  link,
  lstat,
  mkdir,
  mkdtemp,
  readdir,
  readFile,
  rename,
  rm,
  stat,
  utimes,
  writeFile,
} from 'node:fs/promises';
import { tmpdir, userInfo } from 'node:os';
import { basename, extname, join } from 'node:path';

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

/**
 * How long an image nobody uses stays in the cache. Removing one costs the next run that wants it one copy and hash
 * of the executable (about a second for 250 MB); keeping one costs its whole size on disk, and every engine build or
 * upgrade adds one (38 images, 9.3 GB, had piled up). A day keeps the images a user switches between within a day,
 * such as an installed release and a fresh build; an older one is recreated on demand. Age never removes the image
 * being returned or one a live process pinned, so no run loses its executable to pruning.
 */
export const IMAGE_RETENTION_MS = 24 * 60 * 60 * 1000;

const IMAGE = /^[0-9a-f]{64}(?:\.[A-Za-z0-9]+)?$/;

type CacheClock = {
  /** The current time, in milliseconds since the epoch. */
  now?: number;
  /** Whether a process is running. */
  alive?: (pid: number) => boolean;
};

const processAlive = (pid: number) => {
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    return isCode(error, 'EPERM');
  }
};

// Removal is housekeeping: an entry that cannot be removed now stays for the next prune.
const discard = (path: string) =>
  rm(path, { recursive: true, force: true }).catch(() => undefined);

const modified = (path: string) =>
  stat(path).then(
    s => s.mtimeMs,
    () => null
  );

// This process uses an image: used/<image>/<pid>, whose modification time is the last use. A use is recorded before
// the image is checked, so a concurrent prune sees it.
async function recordUse(cache: string, name: string, now: number) {
  const dir = join(cache, 'used', name);
  await mkdir(dir, { recursive: true, mode: 0o700 });
  const record = join(dir, String(process.pid));
  await writeFile(record, '');
  await utimes(record, now / 1000, now / 1000);
}

// An image's recorded uses: whether a live process recorded one, and the last use. Records of exited processes
// past the retention period are dropped on the way.
async function uses(
  cache: string,
  name: string,
  now: number,
  alive: (pid: number) => boolean
) {
  const dir = join(cache, 'used', name);
  let last: number | null = null;
  for (const record of await readdir(dir).catch(() => [] as string[])) {
    const pid = Number(record);
    if (Number.isInteger(pid) && pid > 0 && alive(pid))
      return { live: true, last };
    const time = await modified(join(dir, record));
    if (time === null) continue;
    if (now - time > IMAGE_RETENTION_MS) await discard(join(dir, record));
    last = Math.max(last ?? time, time);
  }
  return { live: false, last };
}

/**
 * Removes from `cache` every image except `keep` that no live process uses and nobody used within the retention
 * period, with its use records and digest stamps; staging copies whose process exited (or, for unnamed ones, older
 * than the retention period); and stamps and records of images no longer present once they are that old.
 */
export async function pruneImages(
  cache: string,
  keep: string,
  { now = Date.now(), alive = processAlive }: CacheClock = {}
) {
  const old = (time: number | null) =>
    time !== null && now - time > IMAGE_RETENTION_MS;
  const images = new Set<string>();
  const removed = new Set<string>();
  for (const name of await readdir(cache)) {
    const path = join(cache, name);
    if (IMAGE.test(name)) {
      const use = path === keep ? null : await uses(cache, name, now, alive);
      // Without a use record (written before records existed), the image's own time is its last use.
      if (!use || use.live || !old(use.last ?? (await modified(path)))) {
        images.add(name);
        continue;
      }
      await discard(path);
      await discard(join(cache, 'used', name));
      removed.add(name.slice(0, 64));
    } else if (name.startsWith('copy-')) {
      const owner = /^copy-(\d+)-/.exec(name)?.[1];
      if (owner ? !alive(Number(owner)) : old(await modified(path)))
        await discard(path);
    }
  }
  const kept = new Set([...images].map(name => name.slice(0, 64)));
  const digests = join(cache, 'digests');
  for (const name of await readdir(digests).catch(() => [] as string[])) {
    const path = join(digests, name);
    const key = await readFile(path, 'utf8').catch(() => '');
    if (removed.has(key) || (!kept.has(key) && old(await modified(path))))
      await discard(path);
  }
  const used = join(cache, 'used');
  for (const name of await readdir(used).catch(() => [] as string[])) {
    if (images.has(name)) continue;
    const use = await uses(cache, name, now, alive);
    if (!use.live && (use.last === null || old(use.last)))
      await discard(join(used, name));
  }
}

// An immutable image per binary content keeps the native server warm between runs.
// Publish with a hard link from a private copy, so concurrent builders never replace a live image.
// Each call records this process's use of the image it returns and prunes the images nobody uses.
export async function freezeExecutable(
  source: string,
  cache = join(tmpdir(), `octocode-graph-cli-${userInfo().uid}`),
  clock: CacheClock = {}
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
  await recordUse(cache, basename(image), clock.now ?? Date.now());
  // Pruning before publishing frees the space a new copy needs.
  await pruneImages(cache, image, clock);
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
  // Named by this process, so a prune removes it only once the process has exited.
  const staging = await mkdtemp(join(cache, `copy-${process.pid}-`));
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
