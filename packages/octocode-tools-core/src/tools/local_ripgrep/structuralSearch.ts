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
import { budgetCaptures } from './captureBudget.js';

const STRUCTURAL_MATCH_VALUE_MAX_CHARS = 300;

function compactStructuralMatchValue(value: string): string {
  const normalized = value.replace(/\s+/g, ' ').trim();
  return normalized.length > STRUCTURAL_MATCH_VALUE_MAX_CHARS
    ? `${normalized.slice(0, STRUCTURAL_MATCH_VALUE_MAX_CHARS - 1)}…`
    : normalized;
}
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
 * The #1 structural miss is a function/method pattern that omits the return
 * type: the natural `function $NAME($$$ARGS) { $$$BODY }` matches 0 real
 * functions because production code carries a return type between `)` and `{`.
 * When the pattern has a parameter list directly followed by a body brace and
 * no return-type position, return the typed variant (insert `: $R`); otherwise
 * undefined. Used both to auto-retry and, as a fallback, to suggest the variant.
 */
function relaxedFunctionReturnTypePattern(
  pattern: string | undefined
): string | undefined {
  if (!pattern || !/\)\s*\{/.test(pattern)) return undefined;
  const relaxed = pattern.replace(/\)\s*\{/, '): $R {');
  return relaxed === pattern ? undefined : relaxed;
}

function relaxedFunctionPatternSuggestion(pattern: string | undefined): string {
  const relaxed = relaxedFunctionReturnTypePattern(pattern);
  return relaxed ? ` Try: \`${relaxed}\`.` : '';
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
  query: RipgrepQuery,
  patternOverride?: string
): Promise<Awaited<ReturnType<typeof contextUtils.structuralSearchFiles>>> {
  const content = await readFile(path, 'utf8');
  const matches = await contextUtils.structuralSearch(
    content,
    path,
    patternOverride ?? query.pattern,
    query.rule
  );

  return {
    files: matches.length > 0 ? [{ path, matches }] : [],
    totalMatches: matches.length,
    parsedFiles: 1,
    skippedByPreFilter: 0,
    skippedUnsupported: 0,
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

  const targetIsFile = await isRegularFile(pathValidation.sanitizedPath);
  const captureText = Boolean((query as { captureText?: boolean }).captureText);
  const buildFilesOptions = (patternOverride?: string) => ({
    path: pathValidation.sanitizedPath,
    pattern: patternOverride ?? query.pattern,
    rule: query.rule,
    // Honor langType by scoping to its extensions when no explicit include
    // was given; explicit include globs always win.
    ...(deriveInclude(query) ? { include: deriveInclude(query) } : {}),
    // Scope parity: forward every scope field the text lane forwards, so
    // `exclude`/`hidden`/`noIgnore`/`maxDepth` are honored on AST search
    // (previously silently dropped — typed-contract violation).
    ...(query.exclude?.length ? { exclude: query.exclude } : {}),
    ...(query.excludeDir?.length
      ? { excludeDir: query.excludeDir }
      : DEFAULT_STRUCTURAL_EXCLUDE_DIRS.length
        ? { excludeDir: DEFAULT_STRUCTURAL_EXCLUDE_DIRS }
        : {}),
    ...(query.hidden !== undefined ? { hidden: query.hidden } : {}),
    ...(query.noIgnore !== undefined ? { noIgnore: query.noIgnore } : {}),
    // maxDepth is deliberately NOT forwarded: the engine's anchored-prefilter
    // path accepts it "for API uniformity" but cannot enforce it (ripgrep-native
    // has no max_depth — see engine structural/files.rs). Enforced uniformly
    // below in the tool layer instead, so behavior matches the schema contract
    // on every structural path.
    maxFiles: query.maxFiles ?? DEFAULT_MAX_STRUCTURAL_FILES,
    maxFileBytes: MAX_STRUCTURAL_FILE_BYTES,
  });
  const runNative = (patternOverride?: string) =>
    targetIsFile
      ? searchSingleFile(pathValidation.sanitizedPath, query, patternOverride)
      : contextUtils.structuralSearchFiles(buildFilesOptions(patternOverride));

  let nativeResult: Awaited<
    ReturnType<typeof contextUtils.structuralSearchFiles>
  >;
  let semicolonNormalized = false;
  let returnTypeRelaxed: string | undefined;
  try {
    nativeResult = await runNative();
    // Statement-level patterns (const/let/var/return/…) must parse as complete
    // statements; without a terminator the bare form parses as an expression
    // fragment and silently matches nothing (`const $X = $Y` → 0 while
    // `const $X = $Y;` matches). Retry once with ';' before reporting zero.
    if (
      nativeResult.totalMatches === 0 &&
      query.pattern &&
      !query.rule &&
      !/[;}]\s*$/.test(query.pattern)
    ) {
      try {
        const retried = await runNative(`${query.pattern};`);
        if (retried.totalMatches > 0) {
          nativeResult = retried;
          semicolonNormalized = true;
        }
      } catch {
        // Terminator retry is best-effort — keep the original zero-match result.
      }
    }
    // Same failure mode for declarations: the natural
    // `function $NAME($$$ARGS) { $$$BODY }` matches 0 real functions because
    // production code carries a return type between `)` and `{`. Retry once
    // with a return-type metavar inserted (`): $R {`) so the bare pattern
    // matches typed functions. Only fires on 0 matches and only when the
    // pattern lacks a return-type position, so it can never override or change
    // an existing positive result.
    if (nativeResult.totalMatches === 0 && query.pattern && !query.rule) {
      const relaxed = relaxedFunctionReturnTypePattern(query.pattern);
      if (relaxed) {
        try {
          const retried = await runNative(relaxed);
          if (retried.totalMatches > 0) {
            nativeResult = retried;
            returnTypeRelaxed = relaxed;
          }
        } catch {
          // Return-type retry is best-effort — keep the original zero-match result.
        }
      }
    }
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
        value: compactStructuralMatchValue(match.text),
        column: match.startCol,
        endColumn: match.endCol,
        // Capture budget: `$$$` list captures (function bodies, arg lists)
        // can dump entire function bodies twice (metavars text + ranges) — a
        // token bomb. Default: keep single-capture text (cheap LSP anchors),
        // drop list-capture text from `metavars`, prune comment-only entries
        // and truncate long texts in `metavarRanges`. `captureText:true`
        // restores verbatim passthrough.
        ...budgetCaptures(match.metavars, match.metavarRanges, captureText),
      })),
    })
  );

  // Depth guard — enforced here because the engine's anchored-prefilter path
  // cannot honor maxDepth natively. Semantics match localViewStructure:
  // 0 = files directly in the search root, 1 = plus one directory level, ….
  const maxDepth = (query as { maxDepth?: number }).maxDepth;
  let depthFiltered = files;
  let depthDropped = 0;
  if (typeof maxDepth === 'number' && !targetIsFile) {
    const root = pathValidation.sanitizedPath.replace(/\/+$/, '');
    depthFiltered = files.filter(f => {
      const rel = f.path.startsWith(`${root}/`)
        ? f.path.slice(root.length + 1)
        : f.path;
      return rel.split('/').length <= maxDepth + 1;
    });
    depthDropped = files.length - depthFiltered.length;
  }
  const totalAfterDepth = depthFiltered.reduce(
    (sum, f) => sum + (f.matchCount ?? f.matches?.length ?? 0),
    0
  );

  // The structural maxFiles cap is LOSSY (the walker stops collecting
  // candidates at the cap). When every unit of the budget was consumed the
  // reported totals may understate reality — surface it as a typed stat.
  const effectiveMaxFiles = query.maxFiles ?? DEFAULT_MAX_STRUCTURAL_FILES;
  const candidatesConsumed =
    (nativeResult.parsedFiles ?? 0) +
    (nativeResult.skippedUnreadable ?? 0) +
    (nativeResult.skippedLarge ?? 0);
  const capReached = candidatesConsumed >= effectiveMaxFiles;

  const stats: SearchStats = {
    totalStructuralMatches:
      typeof maxDepth === 'number'
        ? totalAfterDepth
        : nativeResult.totalMatches,
    ...(capReached ? { capReached: true } : {}),
  };
  // A successful-but-empty structural search is almost always an incomplete
  // pattern; surface remediation through the typed warnings channel (not hints).
  const warnings = [...nativeResult.warnings];
  if (depthDropped > 0) {
    warnings.push(
      `maxDepth ${maxDepth}: dropped ${depthDropped} deeper file(s) — raise maxDepth or omit it to include them.`
    );
  }
  if (semicolonNormalized) {
    warnings.push(
      `Matched after appending ';' — statement patterns must parse as complete statements; the bare pattern parsed as an expression fragment. Results are for "${query.pattern};".`
    );
  }
  if (returnTypeRelaxed) {
    warnings.push(
      `Matched after adding a return-type metavar — the bare pattern missed functions that declare a return type. Results are for "${returnTypeRelaxed}".`
    );
  }
  // Zero matches: ask the engine's detailed variant WHY. It returns the query
  // explanation (parsed kind, literal anchor, pre-filter mode) and staged
  // diagnostics with recovery hints — far more actionable than generic advice.
  // Extra engine run happens only in the otherwise-useless zero-match case.
  if (nativeResult.totalMatches === 0 && !targetIsFile) {
    try {
      const detailed =
        contextUtils.structuralSearchFilesDetailed(buildFilesOptions());
      const q = detailed.query;
      if (q) {
        const parts = [`query parsed as ${q.kind}`];
        if (q.literalAnchor) parts.push(`literal anchor "${q.literalAnchor}"`);
        parts.push(`pre-filter: ${q.preFilter}`);
        if (q.unsafeReason) parts.push(`unsafe: ${q.unsafeReason}`);
        warnings.push(`Engine explanation: ${parts.join(', ')}.`);
        const diagnostics = [
          ...(q.diagnostics ?? []),
          ...(detailed.diagnostics ?? []),
        ];
        for (const d of diagnostics.slice(0, 3)) {
          warnings.push(
            `Engine ${d.severity} [${d.stage}/${d.code}]: ${d.message}${d.recovery ? ` — ${d.recovery}` : ''}`
          );
        }
        if (diagnostics.length > 3) {
          warnings.push(
            `${diagnostics.length - 3} additional engine diagnostic(s) omitted from this summary.`
          );
        }
      }
    } catch {
      // Explanation is best-effort — never fail the response over it.
    }
  }
  // The "complete AST node / use a YAML rule instead" advice only applies to a
  // `pattern`. Don't emit it when the query already uses a `rule` (it would tell
  // a rule author to switch to a rule).
  if (
    (depthFiltered.length === 0 || stats.totalStructuralMatches === 0) &&
    query.pattern &&
    !query.rule &&
    depthDropped === 0
  ) {
    warnings.push(
      ZERO_MATCH_GUIDANCE + relaxedFunctionPatternSuggestion(query.pattern)
    );
  }
  return await buildSearchResult(
    depthFiltered,
    query,
    'structural',
    warnings,
    stats
  );
}
