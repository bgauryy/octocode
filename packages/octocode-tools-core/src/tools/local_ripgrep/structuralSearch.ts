import type { LocalSearchCodeFile } from '@octocodeai/octocode-core/types';
import type { LocalSearchCodeToolResult } from '@octocodeai/octocode-core/extra-types';
import type { StructuralSearchFileResult } from '@octocodeai/octocode-engine';

import { contextUtils } from '../../utils/contextUtils.js';
import {
  validateToolPath,
  createErrorResult,
} from '../../utils/file/toolHelpers.js';
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import type { SearchStats } from '../../utils/core/types.js';
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
 * Native structural search filters candidate files by `include` globs (or scans
 * every supported extension when none are given). `langType` is the ergonomic
 * the regex path uses, so map it to `*.ext` include globs here — otherwise
 * `mode:"structural", langType:"ts"` would also parse HTML/CSS/Scala/etc.
 */
const LANG_TYPE_EXTENSIONS: Record<string, string[]> = {
  ts: ['ts', 'tsx', 'mts', 'cts'],
  typescript: ['ts', 'tsx', 'mts', 'cts'],
  tsx: ['tsx'],
  js: ['js', 'jsx', 'mjs', 'cjs'],
  javascript: ['js', 'jsx', 'mjs', 'cjs'],
  jsx: ['jsx'],
  py: ['py', 'pyi'],
  python: ['py', 'pyi'],
  go: ['go'],
  rs: ['rs'],
  rust: ['rs'],
  java: ['java'],
  c: ['c', 'h'],
  cpp: ['cpp', 'hpp', 'cc', 'cxx', 'hh', 'hxx'],
  'c++': ['cpp', 'hpp', 'cc', 'cxx', 'hh', 'hxx'],
  cs: ['cs'],
  csharp: ['cs'],
  sh: ['sh', 'bash', 'zsh'],
  bash: ['sh', 'bash', 'zsh'],
  shell: ['sh', 'bash', 'zsh'],
  html: ['html', 'htm'],
  css: ['css'],
  scss: ['scss'],
  less: ['less'],
  scala: ['scala', 'sc', 'sbt'],
  json: ['json', 'jsonc'],
  yaml: ['yaml', 'yml'],
  yml: ['yaml', 'yml'],
  toml: ['toml'],
};

function includeGlobsForLangType(langType?: string): string[] | undefined {
  if (!langType) return undefined;
  const key = langType.trim().toLowerCase();
  const exts = LANG_TYPE_EXTENSIONS[key] ?? [key.replace(/^[.*]+/, '')];
  return exts.filter(Boolean).map(ext => `*.${ext}`);
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
    nativeResult = contextUtils.structuralSearchFiles({
      path: pathValidation.sanitizedPath,
      pattern: query.pattern,
      rule: query.rule,
      // Honor langType by scoping to its extensions when no explicit include was
      // given; explicit include globs always win.
      include: query.include?.length
        ? query.include
        : includeGlobsForLangType(query.langType),
      excludeDir: query.excludeDir?.length
        ? query.excludeDir
        : DEFAULT_STRUCTURAL_EXCLUDE_DIRS,
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
        value: match.text.split('\n', 1)[0],
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
  if (files.length === 0 || nativeResult.totalMatches === 0) {
    warnings.push(ZERO_MATCH_GUIDANCE);
  }
  return await buildSearchResult(files, query, 'structural', warnings, stats);
}
