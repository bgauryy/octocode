import { existsSync } from 'node:fs';
import path from 'node:path';

export type LocalRef = { kind: 'local'; path: string };
export type GithubRef = {
  kind: 'github';
  owner: string;
  repo: string;
  /** Sub-path inside the repo, e.g. "src/index.ts" or "" for root */
  subpath: string;
  branch?: string;
  /** Full raw input for error messages */
  raw: string;
};
export type Ref = LocalRef | GithubRef;

/**
 * Parse a full GitHub URL or shorthand into a GithubRef, or return null when
 * the input does not look like a GitHub reference.
 *
 * Accepted formats:
 *   https://github.com/owner/repo/blob/branch/path/to/file
 *   https://github.com/owner/repo/tree/branch/path
 *   https://github.com/owner/repo              (root, no path)
 *   owner/repo/path/to/file
 *   owner/repo@branch/path/to/file
 *   owner/repo                                  (root, no path)
 */
function parseGithubRef(input: string): GithubRef | null {
  const trimmed = input.trim();

  // Full GitHub URL
  const urlMatch = trimmed.match(
    /^https?:\/\/github\.com\/([^/]+)\/([^/]+)(?:\/(blob|tree|raw)\/([^/]+)(?:\/(.+))?)?$/
  );
  if (urlMatch) {
    const [, owner, repo, , branch, subpath] = urlMatch;
    if (owner && repo) {
      return {
        kind: 'github',
        owner,
        repo,
        subpath: subpath ?? '',
        branch: branch ?? undefined,
        raw: trimmed,
      };
    }
  }

  // owner/repo@branch/subpath  or  owner/repo@branch
  const atMatch = trimmed.match(/^([^/]+)\/([^/@]+)@([^/]+)(?:\/(.+))?$/);
  if (atMatch) {
    const [, owner, repo, branch, subpath] = atMatch;
    if (owner && repo) {
      return {
        kind: 'github',
        owner,
        repo,
        subpath: subpath ?? '',
        branch: branch ?? undefined,
        raw: trimmed,
      };
    }
  }

  // owner/repo/subpath  or  owner/repo
  const parts = trimmed.split('/');
  if (
    parts.length >= 2 &&
    // Guard: don't treat absolute paths as GitHub
    !trimmed.startsWith('/') &&
    !trimmed.startsWith('.') &&
    // Heuristic: owner and repo must look like slug identifiers
    /^[a-zA-Z0-9_.-]+$/.test(parts[0]) &&
    /^[a-zA-Z0-9_.-]+$/.test(parts[1])
  ) {
    const owner = parts[0];
    const repo = parts[1];
    const subpath = parts.slice(2).join('/');
    return { kind: 'github', owner, repo, subpath, raw: trimmed };
  }

  return null;
}

/**
 * Resolve a user-supplied path/reference to a typed Ref.
 *
 * Routing priority:
 *   1. Starts with `/`, `./`, or `../` → always local
 *   2. Starts with `http` → always GitHub URL
 *   3. File/dir exists locally (resolved from cwd) → local
 *   4. Matches owner/repo pattern → GitHub
 *   5. Fall back to local (will fail with a clear FS error)
 */
export function resolveRef(input: string, branchOverride?: string): Ref {
  const trimmed = input.trim();

  // Explicit local indicators
  if (
    trimmed.startsWith('/') ||
    trimmed.startsWith('./') ||
    trimmed.startsWith('../')
  ) {
    return { kind: 'local', path: path.resolve(trimmed) };
  }

  // Explicit GitHub URL
  if (trimmed.startsWith('http')) {
    const gh = parseGithubRef(trimmed);
    if (gh) return branchOverride ? { ...gh, branch: branchOverride } : gh;
  }

  // Local existence check (relative to cwd or absolute)
  const localPath = path.resolve(trimmed);
  if (existsSync(localPath)) {
    return { kind: 'local', path: localPath };
  }

  // GitHub pattern
  const gh = parseGithubRef(trimmed);
  if (gh) return branchOverride ? { ...gh, branch: branchOverride } : gh;

  // Default: local (will surface a FS error with a useful message)
  return { kind: 'local', path: localPath };
}

export function isGithubRef(ref: Ref): ref is GithubRef {
  return ref.kind === 'github';
}

export function isLocalRef(ref: Ref): ref is LocalRef {
  return ref.kind === 'local';
}

/** Human-readable label for a ref — used in spinner messages */
export function refLabel(ref: Ref): string {
  if (ref.kind === 'local') return ref.path;
  const branch = ref.branch ? `@${ref.branch}` : '';
  const sub = ref.subpath ? `/${ref.subpath}` : '';
  return `${ref.owner}/${ref.repo}${sub}${branch}`;
}
