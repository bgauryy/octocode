/**
 * Pattern Matching Fallback for Find References
 *
 * Contains pattern matching and workspace search fallback when LSP is not available.
 * Uses lazy enhancement: search → filter → paginate → enhance only visible page.
 *
 * @module tools/lsp_find_references/lspReferencesPatterns
 */

import * as path from 'path';
import { safeReadFile } from '../../lsp/validation.js';

import type {
  FindReferencesResult,
  ReferenceLocation,
  LSPRange,
  LSPPaginationInfo,
} from '../../lsp/types.js';
import type { LSPFindReferencesQuery } from '@octocodeai/octocode-core';
import type { WithOptionalMeta } from '../../types/execution.js';
import { getHints } from '../../hints/index.js';
import { RipgrepMatchOnlySchema } from '../../utils/parsers/schemas.js';
import { matchesFilePatterns } from './lspReferencesCore.js';
import { validateCommand } from 'octocode-security-utils/commandValidator';
import { resolveRipgrepBinary } from '../../utils/exec/ripgrepBinary.js';
import { TOOL_NAME } from './constants.js';
const DEFAULT_CODE_EXTENSIONS = [
  'ts',
  'tsx',
  'js',
  'jsx',
  'mjs',
  'cjs',
  'py',
  'pyi',
  'go',
  'rs',
  'java',
  'kt',
  'kts',
  'swift',
  'dart',
  'rb',
  'php',
  'cs',
  'scala',
  'c',
  'cpp',
  'cc',
  'cxx',
  'h',
  'hpp',
  'lua',
] as const;

const RIPGREP_BUILTIN_CODE_TYPES = [
  'ts',
  'js',
  'py',
  'go',
  'rust',
  'java',
  'kotlin',
  'swift',
  'dart',
  'ruby',
  'php',
  'cs',
  'scala',
  'c',
  'cpp',
  'lua',
] as const;

/**
 * Safely extract exit code from exec/spawn errors (Node.js adds `code` to process errors).
 */
function getExecErrorCode(err: unknown): number | undefined {
  if (err && typeof err === 'object' && 'code' in err) {
    const c = (err as { code: unknown }).code;
    return typeof c === 'number' ? c : undefined;
  }
  return undefined;
}

/**
 * Escape regex metacharacters for safe interpolation into RegExp.
 * @internal Exported for testing
 */
export function escapeForRegex(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

// Lazy-load spawn to avoid module-level child_process dependency
const getSpawn = async () => {
  const { spawn } = await import('child_process');
  return spawn;
};

/**
 * Spawn a command with args and collect stdout.
 * Validates command against the security allowlist before execution.
 */
async function spawnCollectOutput(
  command: string,
  args: string[],
  options: { maxBuffer?: number; timeout?: number } = {}
): Promise<{ stdout: string }> {
  const validation = validateCommand(command, args);
  if (!validation.isValid) {
    throw new Error(
      `Command validation failed: ${validation.error || 'Command not allowed'}`
    );
  }

  const spawnFn = await getSpawn();
  const { maxBuffer = 10 * 1024 * 1024, timeout = 30000 } = options;

  return new Promise((resolve, reject) => {
    const child = spawnFn(command, args, {
      stdio: ['ignore', 'pipe', 'pipe'],
      timeout,
      env: {
        ...Object.fromEntries(
          ['PATH', 'HOME', 'USER', 'LANG', 'TERM', 'SHELL'].map(k => [
            k,
            process.env[k],
          ])
        ),
      },
    });

    let stdout = '';
    let totalSize = 0;

    child.stdout?.on('data', (data: Buffer) => {
      totalSize += data.length;
      if (totalSize > maxBuffer) {
        child.kill('SIGKILL');
        reject(new Error('Output size limit exceeded'));
        return;
      }
      stdout += data.toString();
    });

    child.on('close', code => {
      if (code === 0 || code === 1) {
        resolve({ stdout });
      } else {
        reject(
          Object.assign(new Error(`Process exited with code ${code}`), { code })
        );
      }
    });

    child.on('error', reject);
  });
}

/**
 * Raw reference before content enhancement (no file I/O).
 */
interface RawPatternReference {
  uri: string;
  absolutePath: string;
  range: LSPRange;
  lineContent: string;
  isDefinition: boolean;
  lineNumber: number;
}

/**
 * Enhance a raw reference with context lines from file content.
 * Only called for paginated (visible) items to minimize file I/O.
 */
async function enhancePatternReference(
  raw: RawPatternReference,
  contextLines: number
): Promise<ReferenceLocation> {
  let content = raw.lineContent;

  if (contextLines > 0) {
    try {
      const fileContent = await safeReadFile(raw.absolutePath);
      if (!fileContent) throw new Error('Cannot read file');
      const fileLines = fileContent.split('\n');
      const startLine = Math.max(0, raw.lineNumber - 1 - contextLines);
      const endLine = Math.min(fileLines.length, raw.lineNumber + contextLines);
      content = fileLines.slice(startLine, endLine).join('\n');
    } catch {
      // Context lines read failed; keep single-line ripgrep excerpt.
    }
  }

  return {
    uri: raw.absolutePath,
    range: raw.range,
    content,
    isDefinition: raw.isDefinition,
  };
}

/**
 * Fallback: Find references using pattern matching (ripgrep/grep).
 * Applies file pattern filtering and lazy enhancement.
 */
export async function findReferencesWithPatternMatching(
  absolutePath: string,
  workspaceRoot: string,
  query: WithOptionalMeta<LSPFindReferencesQuery>
): Promise<FindReferencesResult> {
  const allRawReferences = await searchReferencesInWorkspace(
    workspaceRoot,
    query.symbolName,
    absolutePath,
    query.includePattern,
    query.excludePattern
  );

  const totalUnfiltered = allRawReferences.length;

  let filteredReferences = allRawReferences;
  if (!query.includeDeclaration) {
    filteredReferences = allRawReferences.filter(ref => !ref.isDefinition);
  }

  const hasFilters =
    query.includePattern?.length || query.excludePattern?.length;
  if (hasFilters) {
    filteredReferences = filteredReferences.filter(ref =>
      matchesFilePatterns(ref.uri, query.includePattern, query.excludePattern)
    );
  }

  const referencesPerPage = query.referencesPerPage ?? 20;
  const page = query.page ?? 1;
  const totalReferences = filteredReferences.length;
  const totalPages = Math.ceil(totalReferences / referencesPerPage);

  if (totalReferences > 0 && page > totalPages) {
    return {
      status: 'empty',
      pagination: {
        currentPage: page,
        totalPages,
        totalResults: totalReferences,
        hasMore: false,
        resultsPerPage: referencesPerPage,
      },
      hasMultipleFiles:
        new Set(filteredReferences.map(ref => ref.uri)).size > 1,
      hints: [
        ...getHints(TOOL_NAME, 'empty'),
        `Requested page ${page} is outside available range (1-${totalPages}).`,
        `Use page=${totalPages} for the last available page.`,
      ],
    };
  }

  const startIndex = (page - 1) * referencesPerPage;
  const endIndex = Math.min(startIndex + referencesPerPage, totalReferences);
  const paginatedRaw = filteredReferences.slice(startIndex, endIndex);

  if (paginatedRaw.length === 0) {
    const emptyHints = [
      ...getHints(TOOL_NAME, 'empty'),
      `No references found for '${query.symbolName}'`,
    ];

    if (hasFilters && totalUnfiltered > 0) {
      emptyHints.push(
        `Found ${totalUnfiltered} reference(s) but none matched the file patterns`
      );
    }

    return {
      status: 'empty',
      hints: emptyHints,
    };
  }

  const contextLines = query.contextLines ?? 2;
  const paginatedReferences = await Promise.all(
    paginatedRaw.map(raw => enhancePatternReference(raw, contextLines))
  );

  const uniqueFiles = new Set(filteredReferences.map(ref => ref.uri));
  const hasMultipleFiles = uniqueFiles.size > 1;

  const pagination: LSPPaginationInfo = {
    currentPage: page,
    totalPages,
    totalResults: totalReferences,
    hasMore: page < totalPages,
    resultsPerPage: referencesPerPage,
  };

  const hints = [
    ...getHints(TOOL_NAME, 'hasResults'),
    `Found ${totalReferences} reference(s) using text search`,
    'Each location = a usage of this symbol; isDefinition=true marks the declaration',
  ];

  if (hasFilters && totalUnfiltered !== totalReferences) {
    hints.push(
      `Filtered: ${totalReferences} of ${totalUnfiltered} total references match patterns.`
    );
  }

  if (pagination.hasMore) {
    hints.push(
      `Showing page ${page} of ${totalPages}. Use page=${page + 1} for more.`
    );
  }

  return {
    status: 'hasResults',
    locations: paginatedReferences,
    pagination,
    hasMultipleFiles,
    hints,
  };
}

/**
 * Build ripgrep glob arguments from include/exclude patterns.
 * @internal Exported for testing
 */
export function buildRipgrepGlobArgs(
  includePattern?: string[],
  excludePattern?: string[]
): string[] {
  const args: string[] = [];
  if (includePattern?.length) {
    for (const pattern of includePattern) {
      args.push('--glob', pattern);
    }
  }
  if (excludePattern?.length) {
    for (const pattern of excludePattern) {
      args.push('--glob', `!${pattern}`);
    }
  }
  return args;
}

export { findWorkspaceRoot } from '../../lsp/workspaceRoot.js';

/**
 * Build ripgrep argv for symbol reference search.
 * Adds "--" to stop option parsing before user-provided symbol/path values.
 * @internal Exported for testing
 */
export function buildRipgrepSearchArgs(
  workspaceRoot: string,
  symbolName: string,
  includePattern?: string[],
  excludePattern?: string[]
): string[] {
  const escapedSymbol = escapeForRegex(symbolName);
  const typeFlags = RIPGREP_BUILTIN_CODE_TYPES.flatMap(type => ['-t', type]);
  return [
    '--json',
    '--line-number',
    '--column',
    '-w',
    ...typeFlags,
    ...buildRipgrepGlobArgs(includePattern, excludePattern),
    '--',
    escapedSymbol,
    workspaceRoot,
  ];
}

/**
 * Build grep include/exclude arguments from patterns.
 * @internal Exported for testing
 */
export function buildGrepFilterArgs(
  includePattern?: string[],
  excludePattern?: string[]
): string {
  const parts: string[] = [];
  if (includePattern?.length) {
    for (const pattern of includePattern) {
      const filename = pattern.replace(/^\*\*\//, '');
      parts.push(`--include="${filename}"`);
    }
  }
  if (excludePattern?.length) {
    for (const pattern of excludePattern) {
      const cleaned = pattern.replace(/^\*\*\//, '').replace(/\/\*\*$/, '');
      if (pattern.includes('/')) {
        parts.push(`--exclude-dir="${cleaned}"`);
      } else {
        parts.push(`--exclude="${cleaned}"`);
      }
    }
  }
  return parts.join(' ');
}

/**
 * Build grep include/exclude arguments as an array (shell-safe).
 * Each flag and its value are separate array elements for use with spawn().
 * @internal Exported for testing
 */
export function buildGrepFilterArgsArray(
  includePattern?: string[],
  excludePattern?: string[]
): string[] {
  const args: string[] = [];
  if (includePattern?.length) {
    for (const pattern of includePattern) {
      const filename = pattern.replace(/^\*\*\//, '');
      args.push(`--include=${filename}`);
    }
  }
  if (excludePattern?.length) {
    for (const pattern of excludePattern) {
      const cleaned = pattern.replace(/^\*\*\//, '').replace(/\/\*\*$/, '');
      if (pattern.includes('/')) {
        args.push(`--exclude-dir=${cleaned}`);
      } else {
        args.push(`--exclude=${cleaned}`);
      }
    }
  }
  return args;
}

/**
 * Build grep argv for symbol reference search.
 * Adds "--" to stop option parsing before user-provided symbol/path values.
 * @internal Exported for testing
 */
export function buildGrepSearchArgs(
  workspaceRoot: string,
  symbolName: string,
  includePattern?: string[],
  excludePattern?: string[]
): string[] {
  const escapedSymbol = escapeForRegex(symbolName);
  const grepArgs: string[] = ['-rn', '-w'];

  if (includePattern?.length || excludePattern?.length) {
    grepArgs.push(...buildGrepFilterArgsArray(includePattern, excludePattern));
    if (!includePattern?.length) {
      for (const ext of DEFAULT_CODE_EXTENSIONS) {
        grepArgs.push(`--include=*.${ext}`);
      }
    }
  } else {
    for (const ext of DEFAULT_CODE_EXTENSIONS) {
      grepArgs.push(`--include=*.${ext}`);
    }
  }

  grepArgs.push('--', escapedSymbol, workspaceRoot);
  return grepArgs;
}

/**
 * Search for references in the workspace using ripgrep.
 * Returns raw references without content enhancement.
 */
async function searchReferencesInWorkspace(
  workspaceRoot: string,
  symbolName: string,
  sourceFilePath: string,
  includePattern?: string[],
  excludePattern?: string[]
): Promise<RawPatternReference[]> {
  const references: RawPatternReference[] = [];
  const escapedSymbol = escapeForRegex(symbolName);
  const rgArgs = buildRipgrepSearchArgs(
    workspaceRoot,
    symbolName,
    includePattern,
    excludePattern
  );

  try {
    const { stdout } = await spawnCollectOutput(
      resolveRipgrepBinary(),
      rgArgs,
      {
        maxBuffer: 10 * 1024 * 1024,
        timeout: 30000,
      }
    );

    const lines = stdout.trim().split('\n').filter(Boolean);

    for (const line of lines) {
      try {
        const raw = JSON.parse(line);
        const validation = RipgrepMatchOnlySchema.safeParse(raw);
        if (!validation.success) continue;
        const parsed = validation.data;
        if (parsed.type === 'match') {
          const match = parsed.data;
          const filePath = match.path.text;
          const lineNumber = match.line_number;
          const lineContent = match.lines.text.replace(/\n$/, '');

          const regex = new RegExp(`\\b${escapedSymbol}\\b`, 'g');
          let matchResult;
          while ((matchResult = regex.exec(lineContent)) !== null) {
            const column = matchResult.index;
            const isDefinition =
              filePath === sourceFilePath &&
              isLikelyDefinition(lineContent, symbolName);

            const range: LSPRange = {
              start: { line: lineNumber - 1, character: column },
              end: {
                line: lineNumber - 1,
                character: column + symbolName.length,
              },
            };

            const relativeUri = path.relative(workspaceRoot, filePath);

            references.push({
              uri: relativeUri,
              absolutePath: filePath,
              range,
              lineContent,
              isDefinition,
              lineNumber,
            });
          }
        }
      } catch {
        // One ripgrep JSON line failed to parse or match; skip that line.
      }
    }
  } catch (error: unknown) {
    const execCode = getExecErrorCode(error);
    if (execCode !== 1) {
      return await searchReferencesWithGrep(
        workspaceRoot,
        symbolName,
        sourceFilePath,
        includePattern,
        excludePattern
      );
    }
  }

  references.sort((a, b) => {
    if (a.isDefinition && !b.isDefinition) return -1;
    if (!a.isDefinition && b.isDefinition) return 1;
    if (a.uri !== b.uri) return a.uri.localeCompare(b.uri);
    return a.range.start.line - b.range.start.line;
  });

  return references;
}

/**
 * Fallback search using grep.
 * Returns raw references without content enhancement.
 */
async function searchReferencesWithGrep(
  workspaceRoot: string,
  symbolName: string,
  sourceFilePath: string,
  includePattern?: string[],
  excludePattern?: string[]
): Promise<RawPatternReference[]> {
  const references: RawPatternReference[] = [];
  const escapedSymbol = escapeForRegex(symbolName);
  const grepArgs = buildGrepSearchArgs(
    workspaceRoot,
    symbolName,
    includePattern,
    excludePattern
  );

  try {
    const { stdout } = await spawnCollectOutput('grep', grepArgs, {
      maxBuffer: 10 * 1024 * 1024,
      timeout: 30000,
    });

    const lines = stdout.trim().split('\n').filter(Boolean);

    for (const line of lines) {
      const colonIndex = line.indexOf(':');
      if (colonIndex === -1) continue;

      const filePath = line.substring(0, colonIndex);
      const rest = line.substring(colonIndex + 1);
      const secondColon = rest.indexOf(':');
      if (secondColon === -1) continue;

      const lineNumber = parseInt(rest.substring(0, secondColon), 10);
      const lineContent = rest.substring(secondColon + 1);

      if (isNaN(lineNumber)) continue;

      const regex = new RegExp(`\\b${escapedSymbol}\\b`, 'g');
      let matchResult;
      while ((matchResult = regex.exec(lineContent)) !== null) {
        const column = matchResult.index;
        const isDefinition =
          filePath === sourceFilePath &&
          isLikelyDefinition(lineContent, symbolName);

        const range: LSPRange = {
          start: { line: lineNumber - 1, character: column },
          end: { line: lineNumber - 1, character: column + symbolName.length },
        };

        const relativeUri = path.relative(workspaceRoot, filePath);

        references.push({
          uri: relativeUri,
          absolutePath: filePath,
          range,
          lineContent,
          isDefinition,
          lineNumber,
        });
      }
    }
  } catch {
    // Grep fallback search failed; sort and return whatever references were collected.
  }

  references.sort((a, b) => {
    if (a.isDefinition && !b.isDefinition) return -1;
    if (!a.isDefinition && b.isDefinition) return 1;
    if (a.uri !== b.uri) return a.uri.localeCompare(b.uri);
    return a.range.start.line - b.range.start.line;
  });

  return references;
}

/**
 * Heuristic to determine if a line is likely a definition
 * @internal Exported for testing
 */
export function isLikelyDefinition(
  lineContent: string,
  symbolName: string
): boolean {
  // Guard against excessive input that could cause ReDoS
  if (lineContent.length > 1000 || symbolName.length > 255) {
    return false;
  }

  const trimmed = lineContent.trim();
  const escapedSymbol = escapeForRegex(symbolName);

  const definitionPatterns = [
    new RegExp(
      `^(export\\s+)?(const|let|var|function|class|interface|type|enum)\\s+${escapedSymbol}\\b`
    ),
    new RegExp(`^(export\\s+)?async\\s+function\\s+${escapedSymbol}\\b`),
    new RegExp(
      `^(export\\s+)?default\\s+(function|class)\\s+${escapedSymbol}\\b`
    ),
    new RegExp(
      `^(public|private|protected|static|async|readonly)?\\s*${escapedSymbol}\\s*[(:=]`
    ),
    new RegExp(`^(def|class|async\\s+def)\\s+${escapedSymbol}\\b`),
    new RegExp(`^${escapedSymbol}\\s*=`),
    new RegExp(`^func\\s+(\\([^)]+\\)\\s+)?${escapedSymbol}\\b`),
    new RegExp(`^(var|const|type)\\s+${escapedSymbol}\\b`),
    new RegExp(
      `^(pub\\s+)?(fn|struct|enum|trait|type|const|static)\\s+${escapedSymbol}\\b`
    ),
  ];

  return definitionPatterns.some(pattern => pattern.test(trimmed));
}
