import fs from 'node:fs';
import path from 'node:path';
import { ROOT } from './workspaces.mjs';

const NATIVE_ROOT = path.join(ROOT, 'packages', 'octocode-native');
// Past this, stale copies of our crates outweigh the rebuild that `clean:cache` costs (2-3 minutes).
export const NATIVE_DEBUG_LIMIT_BYTES = 50 * 1024 ** 3;

/** The dev-profile output Cargo uses for the native workspace, honoring CARGO_TARGET_DIR as Cargo does. */
export function nativeDebugDir(env = process.env) {
  return path.join(path.resolve(NATIVE_ROOT, env.CARGO_TARGET_DIR || 'target'), 'debug');
}

/**
 * Whether the files under `dir` exceed `limit` bytes. Hardlinks count once, because Cargo links the same
 * object files into deps/ and incremental/. A missing directory, or a file removed mid-walk by a concurrent
 * build, counts as empty.
 */
export function exceedsSize(dir, limit) {
  const pending = [dir];
  const linked = new Set();
  let total = 0;
  while (pending.length > 0) {
    const current = pending.pop();
    let entries;
    try {
      entries = fs.readdirSync(current, { withFileTypes: true });
    } catch (error) {
      if (error.code === 'ENOENT' || error.code === 'ENOTDIR') continue;
      throw error;
    }
    for (const entry of entries) {
      const full = path.join(current, entry.name);
      if (entry.isDirectory()) {
        pending.push(full);
        continue;
      }
      if (!entry.isFile()) continue;
      let stat;
      try {
        stat = fs.lstatSync(full);
      } catch (error) {
        if (error.code === 'ENOENT') continue;
        throw error;
      }
      if (stat.nlink > 1) {
        if (linked.has(stat.ino)) continue;
        linked.add(stat.ino);
      }
      total += stat.size;
      if (total > limit) return true;
    }
  }
  return false;
}
