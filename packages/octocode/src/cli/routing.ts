import { existsSync, statSync } from 'node:fs';
import path from 'node:path';

export type LocalRef = { kind: 'local'; path: string };
export type GithubRef = {
  kind: 'github';
  owner: string;
  repo: string;

  subpath: string;
  branch?: string;

  raw: string;
};
export type Ref = LocalRef | GithubRef;

function parseGithubRef(input: string): GithubRef | null {
  const trimmed = input.trim();

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

  // owner/repo@branch[/path] — branch immediately after the repo.
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

  // owner/repo/path@branch — trailing branch after the subpath. This is the
  // form documented in top-level help and emitted by refLabel/cloneCommandFor,
  // so it must round-trip back through the parser.
  const trailingAtMatch = trimmed.match(/^([^/@]+)\/([^/@]+)\/(.+)@([^/@]+)$/);
  if (trailingAtMatch) {
    const [, owner, repo, subpath, branch] = trailingAtMatch;
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

  const parts = trimmed.split('/');
  if (
    parts.length >= 2 &&
    !trimmed.startsWith('/') &&
    !trimmed.startsWith('.') &&
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

export function resolveRef(input: string, branchOverride?: string): Ref {
  const trimmed = input.trim();

  if (
    trimmed.startsWith('/') ||
    trimmed.startsWith('./') ||
    trimmed.startsWith('../')
  ) {
    return { kind: 'local', path: path.resolve(trimmed) };
  }

  if (trimmed.startsWith('http')) {
    const gh = parseGithubRef(trimmed);
    if (gh) return branchOverride ? { ...gh, branch: branchOverride } : gh;
  }

  const localPath = path.resolve(trimmed);
  if (existsSync(localPath)) {
    return { kind: 'local', path: localPath };
  }

  // A path-shaped miss whose LEADING segment is an existing local directory is
  // almost certainly a local path with a typo (e.g. `src/foo/Bar.ts` after a
  // rename) — route it local so the caller reports a precise "file not found"
  // rather than a confusing GitHub "repository not found (owner=src...)". A
  // genuine owner/repo shorthand (e.g. `vercel/next.js/index.js`) has a leading
  // segment that does NOT exist on disk, so it still falls through to GitHub.
  if (trimmed.includes('/')) {
    const firstSegment = trimmed.split('/')[0];
    if (firstSegment) {
      const firstSegmentPath = path.resolve(firstSegment);
      if (
        existsSync(firstSegmentPath) &&
        statSync(firstSegmentPath).isDirectory()
      ) {
        return { kind: 'local', path: localPath };
      }
    }
  }

  const gh = parseGithubRef(trimmed);
  if (gh) return branchOverride ? { ...gh, branch: branchOverride } : gh;

  return { kind: 'local', path: localPath };
}

export function isGithubRef(ref: Ref): ref is GithubRef {
  return ref.kind === 'github';
}

export function isLocalRef(ref: Ref): ref is LocalRef {
  return ref.kind === 'local';
}

export function refLabel(ref: Ref): string {
  if (ref.kind === 'local') return ref.path;
  const branch = ref.branch ? `@${ref.branch}` : '';
  const sub = ref.subpath ? `/${ref.subpath}` : '';
  return `${ref.owner}/${ref.repo}${sub}${branch}`;
}

/**
 * The exact `clone` quick-command that brings a GitHub ref to disk so it can be
 * searched/outlined locally. Preserves the subpath (sparse subtree) and branch
 * the user already typed, so the suggestion is ready to paste — used by the
 * local-only guards (structural grep, `ls --symbols`).
 */
export function cloneCommandFor(ref: GithubRef): string {
  const branch = ref.branch ? `@${ref.branch}` : '';
  const sub = ref.subpath ? `/${ref.subpath}` : '';
  return `clone ${ref.owner}/${ref.repo}${sub}${branch}`;
}
