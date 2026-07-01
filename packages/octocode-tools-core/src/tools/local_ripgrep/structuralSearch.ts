import type { LocalSearchCodeFile } from '@octocodeai/octocode-core/types';
import type { LocalSearchCodeToolResult } from '@octocodeai/octocode-core/extra-types';
import type { StructuralSearchFileResult } from '@octocodeai/octocode-engine';
import { readFile, stat } from 'node:fs/promises';

import { contextUtils } from '../../utils/contextUtils.js';
import {
  validateToolPath,
  createErrorResult,
} from '../../utils/file/toolHelpers.js';
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import type { SearchStats } from '../../utils/core/types.js';
import { toStructuralSearchIncludeGlobs } from '../../shared/languageSelectors.js';
import { buildSearchResult } from './ripgrepResultBuilder.js';
import type { RipgrepQuery } from './scheme.js';

// No directories excluded by default — structural search must not silently
// skip node_modules/build/dist either. Pass `excludeDir` to trim a search.
const DEFAULT_STRUCTURAL_EXCLUDE_DIRS: string[] = [];

const DEFAULT_MAX_STRUCTURAL_FILES = 2000;
const MAX_STRUCTURAL_FILE_BYTES = 1_000_000;

// Guidance appended to the typed `warnings` channel when a structural search
// parses fine but matches nothing — the usual cause is an incomplete pattern.
const ZERO_MATCH_GUIDANCE =
  '0 structural matches. A pattern matches a complete AST node — a class/function usually needs a body (add `$$$BODY`), and Python/TS definitions may carry a return type (`-> $RET:`) or decorators the pattern must include. For partial or relational matches use a YAML `rule` instead of `pattern`.';

/**
 * The #1 structural miss is a function pattern that omits the return type. When
 * the pattern has a parameter list directly followed by a body brace and no
 * return-type annotation, suggest the typed variant (insert `: $R`) as a
 * concrete, copy-pasteable next step appended to ZERO_MATCH_GUIDANCE.
 */
function relaxedFunctionPatternSuggestion(pattern: string | undefined): string {
  if (!pattern || !/\)\s*\{/.test(pattern)) return '';
  const relaxed = pattern.replace(/\)\s*\{/, '): $R {');
  return relaxed === pattern ? '' : ` Try: \`${relaxed}\`.`;
}

/**
 * Resolve the `include` globs for a structural query: explicit include wins;
 * otherwise derive from `langType` (`langType:'ts'` -> `*.ts`+aliases) so
 * `mode:'structural', langType:'ts'` doesn't parse HTML/CSS/Scala/etc.
 */
function deriveInclude(query: RipgrepQuery): string[] | undefined {
  if (query.include?.length) return query.include;
  return toStructuralSearchIncludeGlobs(query.langType);
}

async function isRegularFile(path: string): Promise<boolean> {
  try {
    return (await stat(path)).isFile();
  } catch {
    return false;
  }
}

async function searchSingleFile(
  path: string,
  query: RipgrepQuery
): Promise<ReturnType<typeof contextUtils.structuralSearchFiles>> {
  const content = await readFile(path, 'utf8');
  const matches = contextUtils.structuralSearch(
    content,
    path,
    query.pattern,
    query.rule
  );

  return {
    files: matches.length > 0 ? [{ path, matches }] : [],
    totalMatches: matches.length,
    parsedFiles: 1,
    skippedByPreFilter: 0,
    skippedUnreadable: 0,
    skippedLarge: 0,
    warnings: [],
  };
}

/**
 * mode:"structural" execution path. Path validation and result shaping stay in
 * TypeScript with the rest of localSearchCode; filesystem traversal, file reads,
 * pre-filtering, parsing, and Octocode AST matching run in native Rust.
 */
export async function searchContentStructural(
  query: RipgrepQuery
): Promise<LocalSearchCodeToolResult> {
  const pathValidation = validateToolPath(query, TOOL_NAMES.LOCAL_RIPGREP);
  if (!pathValidation.isValid) {
    return pathValidation.errorResult as LocalSearchCodeToolResult;
  }

  let nativeResult: ReturnType<typeof contextUtils.structuralSearchFiles>;
  try {
    nativeResult = (await isRegularFile(pathValidation.sanitizedPath))
      ? await searchSingleFile(pathValidation.sanitizedPath, query)
      : contextUtils.structuralSearchFiles({
          path: pathValidation.sanitizedPath,
          pattern: query.pattern,
          rule: query.rule,
          // Honor langType by scoping to its extensions when no explicit include
          // was given; explicit include globs always win.
          ...(deriveInclude(query) ? { include: deriveInclude(query) } : {}),
          // Scope parity: forward every OQL `scope` field the text lane forwards,
          // so `exclude`/`hidden`/`noIgnore`/`maxDepth` are honored on AST search
          // (previously silently dropped — typed-contract violation).
          ...(query.exclude?.length ? { exclude: query.exclude } : {}),
          ...(query.excludeDir?.length
            ? { excludeDir: query.excludeDir }
            : DEFAULT_STRUCTURAL_EXCLUDE_DIRS.length
              ? { excludeDir: DEFAULT_STRUCTURAL_EXCLUDE_DIRS }
              : {}),
          ...(query.hidden !== undefined ? { hidden: query.hidden } : {}),
          ...(query.noIgnore !== undefined ? { noIgnore: query.noIgnore } : {}),
          // maxDepth is a localFindFiles concept, not a RipgrepQuery field — the
          // engine walker honors it (StructuralSearchFilesOptions.maxDepth) for
          // direct napi callers, but the localSearchCode query path can't populate it.
          maxFiles: query.maxFiles ?? DEFAULT_MAX_STRUCTURAL_FILES,
          maxFileBytes: MAX_STRUCTURAL_FILE_BYTES,
        });
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    const langType = query.langType || 'source';
    return createErrorResult(
      new Error(
        `Invalid structural ${query.rule ? 'rule' : 'pattern'}: ${message} — patterns must be valid ${langType} and match a complete node; a class/def usually needs a body (add \`$$$BODY\`). Run \`octocode tools localSearchCode --scheme\` for the live schema.`
      ),
      query,
      {
        toolName: TOOL_NAMES.LOCAL_RIPGREP,
      }
    ) as LocalSearchCodeToolResult;
  }

  const files: LocalSearchCodeFile[] = nativeResult.files.map(
    (file: StructuralSearchFileResult) => ({
      path: file.path,
      matchCount: file.matches.length,
      matches: file.matches.map(match => ({
        line: match.startLine,
        endLine: match.endLine,
        // A match can span lines (e.g. a chained call); collapse it to one
        // normalized line so the row shows the whole matched node instead of
        // its first physical line (often just the receiver of a chain).
        value: match.text.replace(/\s+/g, ' ').trim().slice(0, 300),
        column: match.startCol,
        endColumn: match.endCol,
        metavars: match.metavars,
        // Precise per-capture ranges → an agent can feed a capture straight to
        // lspGetSemantics (uri + line) without re-searching for the symbol.
        ...(match.metavarRanges && Object.keys(match.metavarRanges).length > 0
          ? { metavarRanges: match.metavarRanges }
          : {}),
      })),
    })
  );

  const stats: SearchStats = {
    totalStructuralMatches: nativeResult.totalMatches,
  };
  // A successful-but-empty structural search is almost always an incomplete
  // pattern; surface remediation through the typed warnings channel (not hints).
  const warnings = [...nativeResult.warnings];
  // The "complete AST node / use a YAML rule instead" advice only applies to a
  // `pattern`. Don't emit it when the query already uses a `rule` (it would tell
  // a rule author to switch to a rule).
  if (
    (files.length === 0 || nativeResult.totalMatches === 0) &&
    query.pattern &&
    !query.rule
  ) {
    warnings.push(
      ZERO_MATCH_GUIDANCE + relaxedFunctionPatternSuggestion(query.pattern)
    );
  }
  return await buildSearchResult(files, query, 'structural', warnings, stats);
}
