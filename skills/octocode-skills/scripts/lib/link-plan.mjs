// Skill-sync link planning and application: name/destination safety, destination inspection,
// per-vendor plan rows, and atomic symlink writes.
import {
  existsSync,
  lstatSync,
  mkdirSync,
  readFileSync,
  readlinkSync,
  realpathSync,
  renameSync,
  rmSync,
  symlinkSync,
} from 'node:fs';
import {
  basename,
  dirname,
  isAbsolute,
  join,
  relative,
  resolve,
  sep,
} from 'node:path';
import { VENDORS } from './vendors.mjs';

export const isWin = process.platform === 'win32';

export function readFrontmatterName(skillMdPath) {
  if (!existsSync(skillMdPath)) return null;
  const text = readFileSync(skillMdPath, 'utf8');
  const m = text.match(/^---\n([\s\S]*?)\n---/);
  if (!m) return null;
  const name = m[1].match(/^name:\s*(.+)$/m);
  return name ? name[1].trim().replace(/^["']|["']$/g, '') : null;
}

export function assertSafeSkillName(value) {
  const name = String(value || '').trim();
  if (
    !name ||
    name === '.' ||
    name === '..' ||
    name.length > 128 ||
    !/^[A-Za-z0-9][A-Za-z0-9._-]*$/.test(name)
  ) {
    throw new Error(
      `Unsafe skill name: ${name || '(empty)'}. Use one path segment containing only letters, numbers, dot, underscore, or hyphen.`
    );
  }
  return name;
}

export function assertDestinationWithin(destDir, destPath) {
  const root = resolve(destDir);
  const candidate = resolve(destPath);
  const rel = relative(root, candidate);
  if (!rel || rel === '..' || rel.startsWith(`..${sep}`) || isAbsolute(rel)) {
    throw new Error(`Unsafe destination outside vendor skill root: ${candidate}`);
  }
  return candidate;
}

export function destinationPath(destDir, skillName) {
  return assertDestinationWithin(destDir, join(destDir, skillName));
}

export function inspectDest(destPath) {
  let st;
  try { st = lstatSync(destPath); }
  catch (err) { if (err.code === 'ENOENT') return { state: 'missing' }; throw err; }
  if (st.isSymbolicLink()) {
    let target = null;
    try {
      target = readlinkSync(destPath);
    } catch {
      target = '(unreadable)';
    }
    let resolved = null;
    try {
      resolved = realpathSync(destPath);
    } catch {
      resolved = null;
    }
    return { state: 'symlink', target, resolved };
  }
  if (st.isDirectory()) return { state: 'directory' };
  return { state: 'other' };
}

/** True when a destination link's realpath is the source; the source may itself be reached through a symlink. */
export function linksToSource(resolvedDest, sourcePath) {
  return Boolean(resolvedDest) && resolve(resolvedDest) === realpathSync(sourcePath);
}

export function planRows(sourcePath, skillName, platformIds, projectRoot) {
  const rows = [];
  const seenDest = new Map(); // destPath -> first platform

  for (const id of platformIds) {
    const v = VENDORS[id];
    const scopes = [{ scope: 'user', destDir: v.user }];
    if (projectRoot && v.project) {
      scopes.push({ scope: 'project', destDir: join(resolve(projectRoot), v.project) });
    }
    for (const { scope, destDir } of scopes) {
      const destPath = destinationPath(destDir, skillName);
      const existing = inspectDest(destPath);
      const dupOf = seenDest.get(destPath);
      seenDest.set(destPath, seenDest.get(destPath) || id);
      let action = 'symlink';
      let reason = 'create symlink';
      if (dupOf && dupOf !== id) {
        action = 'skip-dup-path';
        reason = `same path as ${dupOf}`;
      } else if (existing.state === 'symlink') {
        const same = linksToSource(existing.resolved, sourcePath);
        if (same) {
          action = 'ok';
          reason = 'already linked to source';
        } else {
          action = 'conflict-symlink';
          reason = `exists → ${existing.target}`;
        }
      } else if (existing.state === 'directory') {
        action = 'conflict-dir';
        reason = 'real directory present';
      } else if (existing.state === 'other') {
        action = 'conflict-other';
        reason = 'non-dir entry present';
      }
      rows.push({
        platform: id,
        label: v.label,
        scope,
        destDir,
        destPath,
        action,
        reason,
        existing,
      });
    }
  }
  return rows;
}

function ensureParent(dir) {
  if (!existsSync(dir)) mkdirSync(dir, { recursive: true });
}

function atomicSymlink(sourcePath, destPath) {
  ensureParent(dirname(destPath));
  const tmp = `${destPath}.sync-tmp-${process.pid}`;
  try {
    rmSync(tmp, { recursive: true, force: true });
  } catch {
    /* ignore */
  }
  try {
    // absolute source so the link stays valid regardless of cwd
    symlinkSync(sourcePath, tmp, isWin ? 'junction' : 'dir');
    renameSync(tmp, destPath);
  } catch (err) {
    try {
      rmSync(tmp, { recursive: true, force: true });
    } catch {
      /* ignore */
    }
    throw err;
  }
}

export function applyRow(row, sourcePath, { force }) {
  const destPath = assertDestinationWithin(row.destDir, row.destPath);
  if (row.action === 'ok' || row.action === 'skip-dup-path') {
    return { ...row, result: 'skipped', detail: row.reason };
  }
  if (row.action === 'symlink') {
    atomicSymlink(sourcePath, destPath);
    return { ...row, result: 'linked', detail: 'created' };
  }
  if (row.action.startsWith('conflict-')) {
    if (!force) {
      return { ...row, result: 'blocked', detail: `${row.reason} (pass --force with --approve)` };
    }
    const canonicalDest = join(realpathSync(dirname(destPath)), basename(destPath));
    const sourceRelative = relative(canonicalDest, realpathSync(sourcePath));
    if (!sourceRelative || (!isAbsolute(sourceRelative) && sourceRelative !== '..' && !sourceRelative.startsWith(`..${sep}`))) {
      return { ...row, result: 'blocked', detail: 'destination contains the source skill' };
    }
    rmSync(destPath, { recursive: true, force: true });
    atomicSymlink(sourcePath, destPath);
    return { ...row, result: 'replaced', detail: 'force replaced with symlink' };
  }
  return { ...row, result: 'skipped', detail: row.reason };
}
