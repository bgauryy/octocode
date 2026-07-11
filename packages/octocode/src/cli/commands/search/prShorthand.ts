import type { ParsedArgs } from '../../types.js';
import { getBool, getString } from '../../options.js';
import { resolveRef, isGithubRef } from '../../routing.js';
import { looksLikeJsonText, looksLikeJsonFile } from './inputParsing.js';
import { normalizeRepoPath } from './corpusResolution.js';
import type { GithubDiffShortcut } from './types.js';

export function hasPullRequestIntent(options: ParsedArgs['options']): boolean {
  return Boolean(
    getString(options, 'pr') ||
    getString(options, 'state') ||
    getString(options, 'label') ||
    getString(options, 'base') ||
    getString(options, 'head') ||
    getString(options, 'draft') ||
    getBool(options, 'draft') ||
    getBool(options, 'comments', 'commits', 'deep')
  );
}

export function isPullRequestShorthand(args: ParsedArgs): boolean {
  const target = getString(args.options, 'target');
  if (target === 'pullRequests') return true;
  if (hasPullRequestIntent(args.options)) return true;
  const targetArg = args.args.find(arg => !arg.startsWith('-'));
  return Boolean(parsePullRequestRef(targetArg, getString(args.options, 'pr')));
}

export function isPullRequestTextQuery(
  args: ParsedArgs,
  value: string
): boolean {
  return !looksLikeJsonText(value) && isPullRequestShorthand(args);
}

export function pullRequestTextQuery(args: ParsedArgs): string | undefined {
  const value = getString(args.options, 'query');
  return value && isPullRequestTextQuery(args, value) ? value : undefined;
}

export function isPullRequestPatchPath(
  args: ParsedArgs,
  value: string
): boolean {
  return isPullRequestShorthand(args) && !looksLikeJsonFile(value);
}

export function pullRequestPatchPath(args: ParsedArgs): string | undefined {
  const value = getString(args.options, 'file');
  return value && isPullRequestPatchPath(args, value) ? value : undefined;
}

export function resolveGithubDiffShortcut(
  positionals: readonly string[],
  options: ParsedArgs['options'],
  explicitTarget: string | undefined,
  fromFlag: boolean,
  repoOption: string | undefined
): GithubDiffShortcut | undefined {
  if (
    explicitTarget !== 'diff' ||
    fromFlag ||
    repoOption ||
    positionals.length < 2
  ) {
    return undefined;
  }

  const branchOverride = getString(options, 'branch') || undefined;
  const base = resolveRef(positionals[0]!, branchOverride);
  const head = resolveRef(positionals[1]!, branchOverride);
  if (!isGithubRef(base) || !isGithubRef(head)) return undefined;
  if (base.owner !== head.owner || base.repo !== head.repo) return undefined;

  const basePath = normalizeRepoPath(base.subpath);
  const headPath = normalizeRepoPath(head.subpath);
  if (!basePath || basePath !== headPath) return undefined;

  return {
    corpus: {
      kind: 'github',
      repo: `${base.owner}/${base.repo}`,
      path: basePath,
      ...(base.branch ? { ref: base.branch } : {}),
    },
    baseRef:
      getString(options, 'base-ref') ||
      getString(options, 'base') ||
      base.branch ||
      '',
    headRef:
      getString(options, 'head-ref') ||
      getString(options, 'head') ||
      head.branch ||
      '',
    path: basePath,
  };
}

export function parsePullRequestRef(
  input: string | undefined,
  prOverride?: string | undefined
): { owner: string; repo: string; prNumber?: number } | undefined {
  if (!input) return undefined;
  const urlMatch = input.match(/github\.com\/([^/]+)\/([^/]+)\/pull\/(\d+)/);
  if (urlMatch) {
    return {
      owner: urlMatch[1]!,
      repo: urlMatch[2]!,
      prNumber: Number.parseInt(urlMatch[3]!, 10),
    };
  }
  const hashMatch = input.match(/^([^/]+)\/([^#/]+)#(\d+)$/);
  if (hashMatch) {
    return {
      owner: hashMatch[1]!,
      repo: hashMatch[2]!,
      prNumber: Number.parseInt(hashMatch[3]!, 10),
    };
  }
  const repoMatch = input.match(/^([^/]+)\/([^/]+)$/);
  if (!repoMatch) return undefined;
  if (!prOverride) return undefined;
  const prNumber =
    prOverride && /^\d+$/.test(prOverride)
      ? Number.parseInt(prOverride, 10)
      : undefined;
  return {
    owner: repoMatch[1]!,
    repo: repoMatch[2]!,
    ...(prNumber !== undefined ? { prNumber } : {}),
  };
}
