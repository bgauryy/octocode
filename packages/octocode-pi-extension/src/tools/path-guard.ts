/**
 * Path-access guard for extension tools that touch the local filesystem
 * (custom `edit` writes, chromeDebug `scriptFile` reads).
 *
 * Mirrors the access model the native Octocode local tools already enforce in the
 * engine: reads/writes are bounded to the home directory plus `ALLOWED_PATHS`
 * (and the working directory / OS temp dir), with symlinks resolved and the REAL
 * target re-validated so a link cannot escape into a blocked location.
 *
 * This is NOT the old WORKSPACE_ROOT cwd-sandbox (deliberately removed) — it is the
 * documented "home + ALLOWED_PATHS" bound, applied consistently to the two tool
 * surfaces that previously bypassed it. `bash` is overridden by Octocode and
 * path-guards redirect/tee/cp/mv write targets; catastrophic patterns are blocked.
 */
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

/** macOS may expose /tmp as a symlink to /private/tmp; allow both spellings. */
function tempRoots(): string[] {
  const roots = new Set<string>();
  const tmp = os.tmpdir();
  roots.add(tmp);
  if (process.platform === 'darwin') {
    roots.add('/tmp');
    roots.add('/private/tmp');
    if (tmp.startsWith('/var/')) roots.add(`/private${tmp}`);
  }
  return [...roots];
}

/** Expand a leading `~` and resolve to an absolute path. */
function expandHome(p: string): string {
  if (p === '~') return os.homedir();
  if (p.startsWith('~/')) return path.join(os.homedir(), p.slice(2));
  return path.resolve(p);
}

/**
 * Resolve symlinks on the deepest existing ancestor of `p`, then re-append the
 * non-existent tail. Lets us validate the REAL location of a file that does not
 * exist yet (a fresh write target) without a link escaping the allowed roots.
 */
function realpathBounded(p: string): string {
  let cur = path.resolve(p);
  const tail: string[] = [];
  // Walk up to the deepest entry that EXISTS on disk. Use lstat (does NOT follow
  // links), not existsSync (which follows and reports false for a BROKEN symlink)
  // — otherwise a dangling `~/allowed/link -> /etc/x` is treated as a nonexistent
  // tail, only its in-bounds parent gets validated, and a symlink-following write
  // escapes the allowed roots. lstat sees the link itself, so we resolve its real
  // target and validate THAT.
  for (;;) {
    let st: fs.Stats | undefined;
    try {
      st = fs.lstatSync(cur);
    } catch {
      st = undefined;
    }
    if (st) {
      if (st.isSymbolicLink()) {
        // Validate where a following write actually lands: resolve the link
        // chain, or (broken link) its literal target relative to the link's dir.
        let target: string;
        try {
          target = fs.realpathSync.native(cur);
        } catch {
          try {
            target = path.resolve(path.dirname(cur), fs.readlinkSync(cur));
          } catch {
            target = cur;
          }
        }
        return tail.length > 0 ? path.join(target, ...tail) : target;
      }
      let realBase: string;
      try {
        realBase = fs.realpathSync.native(cur); // canonicalize any symlinked ancestors
      } catch {
        realBase = cur; // best-effort if realpath fails (e.g. permissions)
      }
      return tail.length > 0 ? path.join(realBase, ...tail) : realBase;
    }
    tail.unshift(path.basename(cur));
    const parent = path.dirname(cur);
    if (parent === cur) return tail.length > 0 ? path.join(cur, ...tail) : cur; // filesystem root
    cur = parent;
  }
}

function isWithin(child: string, root: string): boolean {
  return child === root || child.startsWith(root + path.sep);
}

/** Allowed roots: cwd, home, OS temp dir, and every ALLOWED_PATHS entry. */
function allowedRoots(cwd: string): string[] {
  const raw = [cwd, os.homedir(), ...tempRoots()];
  const extra = (process.env['ALLOWED_PATHS'] ?? '')
    .split(/[:,]/)
    .map((s) => s.trim())
    .filter(Boolean)
    .map(expandHome);
  const resolved: string[] = [];
  for (const root of [...raw, ...extra]) {
    try {
      resolved.push(fs.realpathSync.native(path.resolve(root)));
    } catch {
      resolved.push(path.resolve(root)); // root may not exist yet — keep the literal
    }
  }
  return resolved;
}

/**
 * Throw if `targetPath` is outside the allowed roots. `action` (e.g. "edit",
 * "scriptFile read") is used in the error message. `cwd` defaults to process.cwd().
 */
export function assertPathAllowed(targetPath: string, cwd: string = process.cwd(), action = 'access'): void {
  const real = realpathBounded(targetPath);
  const roots = allowedRoots(cwd);
  if (roots.some((root) => isWithin(real, root))) return;
  throw new Error(
    `${action} blocked: "${targetPath}" is outside the allowed roots ` +
      `(working directory, home directory, OS temp dir, and ALLOWED_PATHS). ` +
      `Add its root to ALLOWED_PATHS (~/.octocode/.env) to permit it.`,
  );
}
