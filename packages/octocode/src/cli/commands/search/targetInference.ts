import { existsSync, statSync } from 'node:fs';
import type { ParsedArgs } from '../../types.js';
import { getBool, getString } from '../../options.js';
import { resolveRef, isGithubRef } from '../../routing.js';
import { hasPullRequestIntent, parsePullRequestRef } from './prShorthand.js';

export function hasTargetIntent(options: ParsedArgs['options']): boolean {
  return Boolean(
    hasPullRequestIntent(options) ||
    getString(options, 'op') ||
    getBool(options, 'symbols') ||
    getBool(options, 'tree') ||
    getBool(options, 'detailed') ||
    getString(options, 'content-view') ||
    getString(options, 'match-string') ||
    getString(options, 'start-line') ||
    getString(options, 'end-line') ||
    getString(options, 'char-offset') ||
    getString(options, 'char-length') ||
    getBool(options, 'full-content') ||
    getBool(options, 'raw') ||
    getString(options, 'uri') ||
    getString(options, 'symbol') ||
    getString(options, 'workspace-root')
  );
}

export function inferTarget(
  args: ParsedArgs,
  targetArg: string | undefined,
  hints: { hasSearchPredicate?: boolean } = {}
): string | undefined {
  const { options } = args;
  if (
    parsePullRequestRef(targetArg, getString(options, 'pr')) ||
    hasPullRequestIntent(options)
  ) {
    return 'pullRequests';
  }
  if (
    getString(options, 'op') ||
    getString(options, 'symbol') ||
    getBool(options, 'symbols')
  )
    return 'semantics';
  if (getBool(options, 'tree')) return 'structure';
  if (
    hasContentIntent(options) ||
    (isLocalFileTarget(targetArg) && !hints.hasSearchPredicate)
  )
    return 'content';
  // A lone path/ref positional (local dir, owner/repo, owner/repo/file) browses
  // or reads rather than text-searches: dir/repo -> structure, file -> content.
  if (!hints.hasSearchPredicate) {
    const corpusTarget = singlePositionalCorpusTarget(targetArg);
    if (corpusTarget) return corpusTarget;
  }
  if (
    getString(options, 'search') === 'path' ||
    getString(options, 'entry') ||
    getString(options, 'ext') ||
    getString(options, 'name', 'filename') ||
    getString(options, 'path-pattern')
  ) {
    return 'files';
  }
  return undefined;
}

export function hasContentIntent(options: ParsedArgs['options']): boolean {
  return Boolean(
    getBool(options, 'raw', 'full-content') ||
    getString(options, 'match-string') ||
    getString(options, 'start-line') ||
    getString(options, 'end-line') ||
    getString(options, 'char-offset') ||
    getString(options, 'char-length') ||
    ['none', 'standard', 'symbols', 'exact', 'compact'].includes(
      getString(options, 'content-view')
    )
  );
}

export function isCorpusLike(value: string | undefined): boolean {
  if (!value) return false;
  const ref = resolveRef(value);
  if (isGithubRef(ref)) return true;
  try {
    return existsSync(ref.path);
  } catch {
    return false;
  }
}

export function isLocalFileTarget(value: string | undefined): boolean {
  if (!value) return false;
  const ref = resolveRef(value);
  if (isGithubRef(ref)) return false;
  try {
    return existsSync(ref.path) && statSync(ref.path).isFile();
  } catch {
    return false;
  }
}

/** A path segment with a file extension (last segment has a `.ext`). */
export function looksLikeFilePath(subpath: string): boolean {
  const base = subpath.split('/').pop() ?? '';
  return /\.[A-Za-z0-9]+$/.test(base);
}

/**
 * A lone positional that is a path/ref (not a search term) routes to a browse/
 * read target instead of a text search — the terse forms the quick commands
 * expose:
 *   existing local directory   -> structure
 *   owner/repo[/dir]           -> structure
 *   owner/repo/file.ext        -> content
 * Local files are handled by isLocalFileTarget -> content. Returns the OQL
 * target, or undefined when the value should be treated as a search term.
 */
export function singlePositionalCorpusTarget(
  value: string | undefined
): string | undefined {
  if (!value) return undefined;
  const ref = resolveRef(value);
  if (isGithubRef(ref)) {
    return ref.subpath && looksLikeFilePath(ref.subpath)
      ? 'content'
      : 'structure';
  }
  try {
    if (existsSync(ref.path) && statSync(ref.path).isDirectory()) {
      return 'structure';
    }
  } catch {
    /* not an existing local directory */
  }
  return undefined;
}

export function isSinglePositionalTarget(
  args: ParsedArgs,
  fromFlag: boolean
): boolean {
  if (fromFlag || args.args.length !== 1) return false;
  const first = args.args[0];
  const target = getString(args.options, 'target');
  if (getString(args.options, 'repo')) {
    return (
      Boolean(target && target !== 'code') || hasTargetIntent(args.options)
    );
  }
  if (
    (!target || target === 'pullRequests' || target === 'diff') &&
    parsePullRequestRef(first, getString(args.options, 'pr'))
  ) {
    return true;
  }
  if (!target && !hasTargetIntent(args.options) && isLocalFileTarget(first)) {
    return true;
  }
  if (
    !target &&
    !hasTargetIntent(args.options) &&
    singlePositionalCorpusTarget(first)
  ) {
    return true;
  }
  if (!target && !hasTargetIntent(args.options)) return false;
  if (target === 'packages' || target === 'repositories') return false;
  // A file-like positional with content intent (e.g. --content-view, line
  // range, --match-string) is the read TARGET even if it doesn't exist yet —
  // otherwise it falls through to a text search with no corpus and resolves to
  // cwd ".", yielding a misleading "Path is a directory" instead of a clean
  // "File not found".
  if (hasContentIntent(args.options) && looksLikeFilePath(first ?? '')) {
    return true;
  }
  return isCorpusLike(first) || target === 'content' || target === 'structure';
}
