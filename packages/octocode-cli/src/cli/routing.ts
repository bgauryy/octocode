import { existsSync } from 'node:fs';
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
