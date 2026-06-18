import { readFile, stat } from 'node:fs/promises';

import type { LocalSearchCodeFile } from '@octocodeai/octocode-core/types';
import type { LocalSearchCodeToolResult } from '@octocodeai/octocode-core/extra-types';

import { contextUtils } from '../../utils/contextUtils.js';
import {
  validateToolPath,
  createErrorResult,
} from '../../utils/file/toolHelpers.js';
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import type { SearchStats } from '../../utils/core/types.js';
import { buildSearchResult } from './ripgrepResultBuilder.js';
import type { RipgrepQuery } from './scheme.js';

// File extensions whose grammar the structural engine ships (mirrors
// octocode-context-utils' signatures/languages.rs LANGUAGE_TABLE). Used only to
// shortlist candidate files before parsing — the engine is the source of truth
// and throws on anything it cannot parse.
const STRUCTURAL_EXTENSIONS = [
  'ts',
  'tsx',
  'js',
  'jsx',
  'mjs',
  'cjs',
  'py',
  'go',
  'rs',
  'java',
  'c',
  'h',
  'cpp',
  'cc',
  'cxx',
  'hpp',
  'hh',
  'hxx',
  'cs',
  'sh',
  'bash',
  'zsh',
] as const;

const DEFAULT_STRUCTURAL_EXCLUDE_DIRS = [
  'node_modules',
  'dist',
  '.git',
  'build',
  'coverage',
  '.next',
  'out',
  'target',
];

// Cap candidate files when none was supplied, so a structural search rooted at
// a large tree cannot fan out unbounded.
const DEFAULT_MAX_STRUCTURAL_FILES = 2000;
// Skip files larger than this from a parse — they are almost never the target
// of a structural query and dominate latency. Matches the engine's own guard.
const MAX_STRUCTURAL_FILE_BYTES = 1_000_000;

/**
 * Derive a literal text anchor from a pattern so ripgrep-style pre-filtering
 * (here, a cheap `content.includes`) can skip the expensive PARSE of files that
 * cannot contain a match. Sound by construction: any literal identifier in the
 * pattern must appear verbatim in every match (only metavars vary). Returns
 * `undefined` for rules or metavar-only patterns → full-corpus parse.
 */
function deriveLiteralAnchor(pattern: string | undefined): string | undefined {
  if (!pattern) return undefined;
  // Strip metavariables ($X, $$$ARGS, $_) before extracting literals.
  const withoutMetavars = pattern.replace(/\$+[A-Za-z0-9_]*/g, ' ');
  const literals: string[] =
    withoutMetavars.match(/[A-Za-z_][A-Za-z0-9_]{2,}/g) ?? [];
  // The longest literal is the most selective anchor.
  let anchor: string | undefined;
  for (const literal of literals) {
    if (!anchor || literal.length > anchor.length) anchor = literal;
  }
  return anchor;
}

async function collectCandidateFiles(
  rootPath: string,
  query: RipgrepQuery
): Promise<string[]> {
  const stats = await stat(rootPath);
  if (stats.isFile()) {
    return [rootPath];
  }

  const include = query.include?.length ? query.include : undefined;
  const names = include ?? STRUCTURAL_EXTENSIONS.map(ext => `*.${ext}`);
  const excludeDir = query.excludeDir?.length
    ? query.excludeDir
    : DEFAULT_STRUCTURAL_EXCLUDE_DIRS;

  const result = contextUtils.queryFileSystem({
    path: rootPath,
    recursive: true,
    entryType: 'f',
    names,
    excludeDir,
    limit: query.maxFiles ?? DEFAULT_MAX_STRUCTURAL_FILES,
  });

  return result.entries.map(entry => entry.path);
}

/**
 * mode:"structural" execution path. Resolves candidate files, pre-filters them
 * with a literal anchor when the pattern provides one, runs the AST engine per
 * file, and reuses the ripgrep result builder so the output shape (pagination,
 * sizing) is identical to the other localSearchCode modes.
 */
export async function searchContentStructural(
  query: RipgrepQuery
): Promise<LocalSearchCodeToolResult> {
  const pathValidation = validateToolPath(query, TOOL_NAMES.LOCAL_RIPGREP);
  if (!pathValidation.isValid) {
    return pathValidation.errorResult as LocalSearchCodeToolResult;
  }
  const rootPath = pathValidation.sanitizedPath;

  const pattern = query.pattern;
  const rule = query.rule;
  const anchor = deriveLiteralAnchor(pattern);

  let candidateFiles: string[];
  try {
    candidateFiles = await collectCandidateFiles(rootPath, query);
  } catch (error) {
    return createErrorResult(error, query, {
      toolName: TOOL_NAMES.LOCAL_RIPGREP,
    }) as LocalSearchCodeToolResult;
  }

  const files: LocalSearchCodeFile[] = [];
  const warnings: string[] = [];
  let totalMatches = 0;
  let parsedFiles = 0;
  let skippedByPreFilter = 0;
  let fatalError: string | undefined;

  for (const filePath of candidateFiles) {
    if (fatalError) break;

    let content: string;
    try {
      const fileStat = await stat(filePath);
      if (fileStat.size > MAX_STRUCTURAL_FILE_BYTES) continue;
      content = await readFile(filePath, 'utf8');
    } catch {
      // Unreadable / vanished file — skip, do not abort the whole search.
      continue;
    }

    // Sound pre-filter: a file lacking the pattern's literal anchor cannot
    // contain a match, so skip the parse entirely (KPI #8).
    if (anchor && !content.includes(anchor)) {
      skippedByPreFilter++;
      continue;
    }

    let matches: ReturnType<typeof contextUtils.structuralSearch>;
    try {
      matches = contextUtils.structuralSearch(content, filePath, pattern, rule);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      // The engine rejects extensions it has no grammar for — skip those files.
      // Any other error is a bad pattern/rule and applies to every file, so
      // surface it once instead of repeating it per file.
      if (message.includes('does not support')) continue;
      fatalError = message;
      break;
    }

    parsedFiles++;
    if (matches.length === 0) continue;

    totalMatches += matches.length;
    files.push({
      path: filePath,
      matchCount: matches.length,
      matches: matches.map(match => ({
        line: match.startLine,
        value: match.text.split('\n', 1)[0],
        column: match.startCol,
      })),
    });
  }

  if (fatalError) {
    return createErrorResult(
      new Error(
        `Invalid structural ${rule ? 'rule' : 'pattern'}: ${fatalError}`
      ),
      query,
      {
        toolName: TOOL_NAMES.LOCAL_RIPGREP,
        customHints: [
          rule
            ? 'Check the YAML rule shape. Relational sub-rules (inside/has) need `stopBy: end` to walk all ancestors/descendants, else they silently match nothing.'
            : 'Check the pattern syntax. Use $X for a single node, $$$ARGS for a list. The pattern must be a complete code fragment for the target language.',
        ],
      }
    ) as LocalSearchCodeToolResult;
  }

  if (!anchor) {
    warnings.push(
      `No literal anchor in the ${rule ? 'rule' : 'pattern'} — parsed all ${parsedFiles} candidate file(s) with no text pre-filter.`
    );
  } else if (skippedByPreFilter > 0) {
    warnings.push(
      `Pre-filter on "${anchor}" skipped parsing ${skippedByPreFilter} file(s); parsed ${parsedFiles}.`
    );
  }

  const stats: SearchStats = { matchCount: totalMatches };
  const result = await buildSearchResult(files, query, 'rg', warnings, stats);

  if (totalMatches > 0) {
    const hints = Array.isArray(result.hints) ? [...result.hints] : [];
    hints.push(
      'Structural matches return node ranges — pass matches[].line as the lspGetSemantics lineHint to navigate semantically.'
    );
    return { ...result, hints } as LocalSearchCodeToolResult;
  }

  return result;
}
