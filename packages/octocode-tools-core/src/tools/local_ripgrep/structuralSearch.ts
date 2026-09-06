import type { LocalSearchCodeFile } from '@octocodeai/octocode-core/types';
import type { LocalSearchCodeToolResult } from '@octocodeai/octocode-core/extra-types';
import type { StructuralSearchFileResult } from '@octocodeai/octocode-engine';
import { readFile, stat } from 'node:fs/promises';

import { contextUtils } from '../../utils/contextUtils.js';
import {
  validateToolPath,
  createErrorResult,
} from '../../utils/file/toolHelpers.js';
import { TOOL_NAMES } from '../toolMetadata/names.js';
import type { SearchStats } from '../../utils/core/types.js';
import { toStructuralSearchIncludeGlobs } from '../../shared/languageSelectors/classify.js';
import { buildSearchResult } from './ripgrepResultBuilder/buildResult.js';
import { budgetCaptures } from './captureBudget.js';
import {
  executionLimitDiagnostic,
  isIncomplete,
  rewrittenQueryDiagnostic,
  withCompleteness,
} from './structuralCompleteness.js';
import type { RipgrepQuery } from './scheme.js';

const STRUCTURAL_MATCH_VALUE_MAX_CHARS = 300;

function compactStructuralMatchValue(value: string): string {
  const normalized = value.replace(/\s+/g, ' ').trim();
  return normalized.length > STRUCTURAL_MATCH_VALUE_MAX_CHARS
    ? `${normalized.slice(0, STRUCTURAL_MATCH_VALUE_MAX_CHARS - 1)}…`
    : normalized;
}

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
 * undefined. Used to auto-retry; successful rewrites are disclosed explicitly.
 */
function relaxedFunctionReturnTypePattern(
  pattern: string | undefined
): string | undefined {
  if (!pattern || !/\)\s*\{/.test(pattern)) return undefined;
  const relaxed = pattern.replace(/\)\s*\{/, '): $R {');
  return relaxed === pattern ? undefined : relaxed;
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
    status: 'ok',
    diagnostics: [],
    scanTruncated: false,
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
 * TypeScript with the rest of local text search; filesystem traversal, file reads,
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
    ...(query.excludeDir?.length ? { excludeDir: query.excludeDir } : {}),
    ...(query.hidden !== undefined ? { hidden: query.hidden } : {}),
    ...(query.noIgnore !== undefined ? { noIgnore: query.noIgnore } : {}),
    // Public depth 0 means direct children; the native walker counts the root
    // as depth 0 and its direct children as depth 1. Filter before the file cap.
    ...(query.maxDepth !== undefined ? { maxDepth: query.maxDepth + 1 } : {}),
    maxFiles: query.maxFiles ?? DEFAULT_MAX_STRUCTURAL_FILES,
    maxFileBytes: MAX_STRUCTURAL_FILE_BYTES,
  });
  let attemptedPattern = query.pattern;
  let effectivePattern = query.pattern;
  const runNative = (patternOverride?: string) => {
    attemptedPattern = patternOverride ?? query.pattern;
    return targetIsFile
      ? searchSingleFile(pathValidation.sanitizedPath, query, patternOverride)
      : contextUtils.structuralSearchFiles(buildFilesOptions(patternOverride));
  };

  let nativeResult: Awaited<
    ReturnType<typeof contextUtils.structuralSearchFiles>
  >;
  try {
    nativeResult = await runNative();
    // Statement-level patterns (const/let/var/return/…) must parse as complete
    // statements; without a terminator the bare form parses as an expression
    // fragment and silently matches nothing (`const $X = $Y` → 0 while
    // `const $X = $Y;` matches). Retry once with ';' before reporting zero.
    if (
      nativeResult.totalMatches === 0 &&
      !isIncomplete(nativeResult) &&
      query.pattern &&
      !query.rule &&
      !/[;}]\s*$/.test(query.pattern)
    ) {
      try {
        const retried = await runNative(`${query.pattern};`);
        if (retried.totalMatches > 0 || isIncomplete(retried)) {
          nativeResult = retried;
          effectivePattern = `${query.pattern};`;
        }
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        if (executionLimitDiagnostic(message, pathValidation.sanitizedPath))
          throw error;
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
    if (
      nativeResult.totalMatches === 0 &&
      !isIncomplete(nativeResult) &&
      query.pattern &&
      !query.rule
    ) {
      const relaxed = relaxedFunctionReturnTypePattern(query.pattern);
      if (relaxed) {
        try {
          const retried = await runNative(relaxed);
          if (retried.totalMatches > 0 || isIncomplete(retried)) {
            nativeResult = retried;
            effectivePattern = relaxed;
          }
        } catch (error) {
          const message =
            error instanceof Error ? error.message : String(error);
          if (executionLimitDiagnostic(message, pathValidation.sanitizedPath))
            throw error;
          // Return-type retry is best-effort — keep the original zero-match result.
        }
      }
    }
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    const nativeCode = /^\[(structural\.[A-Za-z.]+)\]/.exec(message)?.[1];
    if (nativeCode === 'structural.language.unsupported') {
      return createErrorResult(
        `${message}. Use localSearch operation:"text" to search this file.`,
        query,
        {
          toolName: TOOL_NAMES.LOCAL_RIPGREP,
          extra: { errorCode: 'structural.language.unsupported' },
        }
      ) as LocalSearchCodeToolResult;
    }
    const diagnostic = executionLimitDiagnostic(
      message,
      pathValidation.sanitizedPath
    );
    if (diagnostic) {
      const rewritten = rewrittenQueryDiagnostic(
        query.pattern,
        attemptedPattern,
        pathValidation.sanitizedPath
      );
      return withCompleteness(
        await buildSearchResult([], query, 'structural', [diagnostic.message], {
          totalStructuralMatches: 0,
        }),
        'truncated',
        [diagnostic, ...(rewritten ? [rewritten] : [])]
      );
    }
    const langType = query.langType || 'source';
    return createErrorResult(
      new Error(
        `Invalid structural ${query.rule ? 'rule' : 'pattern'}: ${message} — use valid ${langType} and match a complete node; a class/def usually needs a body (add \`$$$BODY\`). Run \`octocode tools localSearch --scheme\` for the live schema.`
      ),
      query,
      {
        toolName: TOOL_NAMES.LOCAL_RIPGREP,
        ...(nativeCode ? { extra: { errorCode: nativeCode } } : {}),
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

  // Defense-in-depth after the native candidate filter. Semantics match the local tree operation:
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

  // Native collection probes one extra candidate to distinguish an exact-cap
  // complete result from a scan with reachable files beyond the budget.
  const effectiveMaxFiles = query.maxFiles ?? DEFAULT_MAX_STRUCTURAL_FILES;
  const capReached = !targetIsFile && nativeResult.scanTruncated === true;

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
  const diagnostics = [...(nativeResult.diagnostics ?? [])];
  const rewritten = rewrittenQueryDiagnostic(
    query.pattern,
    effectivePattern,
    pathValidation.sanitizedPath
  );
  if (rewritten) diagnostics.push(rewritten);
  if (depthDropped > 0) {
    warnings.push(
      `maxDepth ${maxDepth}: dropped ${depthDropped} deeper file(s) — raise maxDepth or omit it to include them.`
    );
  }
  // Zero matches: explain the query plan returned by the same asynchronous
  // native scan. Re-running the synchronous detailed directory search here
  // blocked the JavaScript event loop and repeated all filesystem/parse work.
  if (
    nativeResult.totalMatches === 0 &&
    !isIncomplete(nativeResult) &&
    !targetIsFile
  ) {
    const q = nativeResult.query;
    if (q) {
      const parts = [`query parsed as ${q.kind}`];
      if (q.literalAnchor) parts.push(`literal anchor "${q.literalAnchor}"`);
      parts.push(`pre-filter: ${q.preFilter}`);
      if (q.unsafeReason) parts.push(`unsafe: ${q.unsafeReason}`);
      warnings.push(`Engine explanation: ${parts.join(', ')}.`);
      const explanationDiagnostics = q.diagnostics ?? [];
      diagnostics.push(...explanationDiagnostics);
      for (const d of explanationDiagnostics.slice(0, 3)) {
        warnings.push(
          `Engine ${d.severity} [${d.stage}/${d.code}]: ${d.message}${d.recovery ? ` — ${d.recovery}` : ''}`
        );
      }
      if (explanationDiagnostics.length > 3) {
        warnings.push(
          `${explanationDiagnostics.length - 3} additional engine diagnostic(s) omitted from this summary.`
        );
      }
    }
  }
  // The "complete AST node / use a YAML rule instead" advice only applies to a
  // `pattern`. Don't emit it when the query already uses a `rule` (it would tell
  // a rule author to switch to a rule).
  if (
    (depthFiltered.length === 0 || stats.totalStructuralMatches === 0) &&
    !isIncomplete(nativeResult) &&
    query.pattern &&
    !query.rule &&
    depthDropped === 0
  ) {
    // The return-type variant was already attempted above. Suggesting it again
    // after zero matches is redundant and may use another language's syntax.
    const message = ZERO_MATCH_GUIDANCE;
    warnings.push(message);
    diagnostics.push({
      code: 'structural.query.noMatches',
      severity: 'info',
      stage: 'match',
      message,
      path: pathValidation.sanitizedPath,
    });
  }
  return withCompleteness(
    await buildSearchResult(
      depthFiltered,
      { ...query, pattern: effectivePattern, maxFiles: effectiveMaxFiles },
      'structural',
      warnings,
      stats
    ),
    nativeResult.status,
    diagnostics
  );
}
