import path from 'node:path';
import { existsSync, statSync } from 'node:fs';
import { getBool, getString } from '../options.js';
import { c, bold, dim } from '../../utils/colors.js';
import { EXIT } from '../exit-codes.js';
import { executeDirectTool } from '@octocodeai/octocode-tools-core/direct';
import { getDirectToolText } from './direct-tool-output.js';

// Semantic symbol outline (LSP documentSymbols). This is the structure-of-a-file
// view used by `search --symbols` for a file or directory. Local-only (ast/LSP
// can't run on GitHub).

const DEFAULT_SOURCE_EXTENSIONS = [
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
  'kt',
  'swift',
  'cs',
  'cpp',
  'c',
  'h',
  'hpp',
  'php',
  'rb',
  'lua',
  'dart',
] as const;

type DirectToolExecutor = typeof executeDirectTool;

type FileEntry = { readonly path?: unknown };

type SymbolEntry = {
  readonly name?: unknown;
  readonly kind?: unknown;
  readonly line?: unknown;
  readonly containerName?: unknown;
};

type RenderedFile = {
  readonly uri: string;
  readonly symbols: SymbolEntry[];
  readonly totalSymbols?: number;
};

type OutlineExtraOutput = {
  readonly structured?: Record<string, unknown>;
  readonly text?: string;
};

function parseExtensions(value: string): string[] {
  if (!value) return [...DEFAULT_SOURCE_EXTENSIONS];
  return value
    .split(',')
    .map(item => item.trim().replace(/^\*\./, '').replace(/^\./, ''))
    .filter(Boolean);
}

function sourceNames(extensions: readonly string[]): string[] {
  return extensions.map(ext => `*.${ext}`);
}

function chunk<T>(items: readonly T[], size: number): T[][] {
  const chunks: T[][] = [];
  for (let index = 0; index < items.length; index += size) {
    chunks.push(items.slice(index, index + size));
  }
  return chunks;
}

function resultRecords(value: unknown): Array<{ readonly data?: unknown }> {
  if (!value || typeof value !== 'object') return [];
  const results = (value as { readonly results?: unknown }).results;
  return Array.isArray(results) ? results : [];
}

function filePathsFromFindResult(value: unknown): string[] {
  const files = resultRecords(value).flatMap(result => {
    const data = result.data;
    if (!data || typeof data !== 'object') return [];
    const maybeFiles = (data as { readonly files?: unknown }).files;
    return Array.isArray(maybeFiles) ? maybeFiles : [];
  });

  return files
    .map(file => (file as FileEntry).path)
    .filter((filePath): filePath is string => typeof filePath === 'string');
}

function symbolsFromLspResult(value: unknown): RenderedFile[] {
  return resultRecords(value).flatMap(result => {
    const data = result.data;
    if (!data || typeof data !== 'object') return [];
    const uri = (data as { readonly uri?: unknown }).uri;
    const payload = (data as { readonly payload?: unknown }).payload;
    if (!payload || typeof payload !== 'object') return [];
    const symbols = (payload as { readonly symbols?: unknown }).symbols;
    const totalSymbols = (payload as { readonly totalSymbols?: unknown })
      .totalSymbols;
    return [
      {
        uri: typeof uri === 'string' ? uri : '',
        symbols: Array.isArray(symbols) ? (symbols as SymbolEntry[]) : [],
        totalSymbols:
          typeof totalSymbols === 'number' ? totalSymbols : undefined,
      },
    ];
  });
}

function symbolMatchesKind(symbol: SymbolEntry, kindFilter: string): boolean {
  if (!kindFilter) return true;
  return String(symbol.kind ?? '').toLowerCase() === kindFilter.toLowerCase();
}

function renderSymbols(
  files: readonly RenderedFile[],
  kindFilter: string
): string {
  const lines: string[] = [];

  for (const file of files) {
    const filtered = file.symbols.filter(symbol =>
      symbolMatchesKind(symbol, kindFilter)
    );
    const relative = path.relative(process.cwd(), file.uri) || file.uri;
    lines.push(
      `${c('cyan', bold(relative))}${file.totalSymbols !== undefined ? dim(` (${file.totalSymbols} symbols)`) : ''}`
    );

    if (filtered.length === 0) {
      lines.push(
        `  ${dim(kindFilter ? `No ${kindFilter} symbols found.` : 'No symbols found.')}`
      );
      continue;
    }

    for (const symbol of filtered) {
      const line = typeof symbol.line === 'number' ? symbol.line : '?';
      const kind = String(symbol.kind ?? 'symbol');
      const name = String(symbol.name ?? '<anonymous>');
      const container =
        typeof symbol.containerName === 'string'
          ? dim(`  in ${symbol.containerName}`)
          : '';
      lines.push(
        `  ${c('yellow', `L${line}`)} ${dim(kind.padEnd(12))} ${name}${container}`
      );
    }
  }

  return lines.length > 0 ? lines.join('\n') : dim('No symbols found.');
}

/**
 * localFindFiles relativizes hits against the search root's PARENT, so a search
 * in `.../commands` returns `commands/foo.ts`. Resolving that against the search
 * root itself would double the segment (`commands/commands/foo.ts` → ENOENT), so
 * resolve against the parent first and fall back to the root, guarded by exists.
 */
function resolveDiscoveredPath(dirPath: string, filePath: string): string {
  if (path.isAbsolute(filePath)) return filePath;
  const fromParent = path.resolve(path.dirname(dirPath), filePath);
  if (existsSync(fromParent)) return fromParent;
  return path.resolve(dirPath, filePath);
}

async function discoverSourceFiles(
  exec: DirectToolExecutor,
  dirPath: string,
  extensions: readonly string[],
  limit: number,
  depth: number
): Promise<string[]> {
  const result = await exec('localFindFiles', {
    queries: [
      {
        path: dirPath,
        entryType: 'f',
        names: sourceNames(extensions),
        maxDepth: depth,
        limit,
        itemsPerPage: limit,
        sortBy: 'path',
        mainResearchGoal: 'Discover source files for semantic outline',
        researchGoal: `Find source files in ${dirPath}`,
        reasoning: 'CLI search --symbols directory discovery',
      },
    ],
  });

  if (result.isError) {
    throw new Error(getDirectToolText(result));
  }

  return filePathsFromFindResult(result.structuredContent)
    .map(filePath => resolveDiscoveredPath(dirPath, filePath))
    .slice(0, limit);
}

async function fetchDocumentSymbols(
  exec: DirectToolExecutor,
  filePaths: readonly string[],
  pageSize: number
): Promise<{
  readonly structured: unknown[];
  readonly rendered: RenderedFile[];
}> {
  const structured: unknown[] = [];
  const rendered: RenderedFile[] = [];

  for (const batch of chunk(filePaths, 5)) {
    const result = await exec('lspGetSemantics', {
      queries: batch.map(filePath => ({
        uri: path.resolve(filePath),
        type: 'documentSymbols',
        itemsPerPage: pageSize,
        mainResearchGoal: 'Build semantic symbol outline',
        researchGoal: `List document symbols in ${filePath}`,
        reasoning: 'CLI search --symbols',
      })),
    });

    if (result.isError) {
      throw new Error(getDirectToolText(result));
    }

    structured.push(result.structuredContent);
    rendered.push(...symbolsFromLspResult(result.structuredContent));
  }

  return { structured, rendered };
}

function parsePositiveInt(value: string, fallback: number): number {
  if (!value) return fallback;
  const parsed = Number.parseInt(value, 10);
  return Number.isInteger(parsed) && parsed > 0 ? parsed : fallback;
}

/**
 * Print a semantic symbol outline for `target` (a local file or directory).
 * Shared outline mode for `search --symbols`. Sets `process.exitCode` on
 * failure and returns.
 */
export async function outlineSymbols(
  target: string,
  options: Record<string, string | boolean>,
  extraOutput?: OutlineExtraOutput
): Promise<void> {
  const jsonOutput = getBool(options, 'json');
  const kindFilter = getString(options, 'kind');
  const extensions = parseExtensions(getString(options, 'ext'));
  const limit = parsePositiveInt(getString(options, 'limit'), 10);
  const depth = parsePositiveInt(getString(options, 'depth'), 4);
  // An outline should show the whole file, not a 40-symbol slice — otherwise
  // symbols that sort late (e.g. functions after a block of interface
  // properties) fall onto page 2 and `--kind` filtering, which runs over the
  // returned page, silently misses them. Pull a generous page; widen further
  // when a kind filter is active so the filter sees every symbol.
  const requestedPageSize = parsePositiveInt(
    getString(options, 'page-size'),
    200
  );
  const pageSize = kindFilter
    ? Math.max(requestedPageSize, 1000)
    : requestedPageSize;

  const resolved = path.resolve(target);
  if (!existsSync(resolved)) {
    const err = `Path not found: ${target}`;
    if (jsonOutput) console.log(JSON.stringify({ success: false, error: err }));
    else console.error(`\n  ${c('red', '✗')} ${err}\n`);
    process.exitCode = EXIT.USAGE;
    return;
  }

  try {
    const stats = statSync(resolved);
    const files = stats.isDirectory()
      ? await discoverSourceFiles(
          executeDirectTool,
          resolved,
          extensions,
          limit,
          depth
        )
      : [resolved];

    if (files.length === 0) {
      if (jsonOutput) {
        console.log(
          JSON.stringify(
            { files: [], symbols: [], ...(extraOutput?.structured ?? {}) },
            null,
            2
          )
        );
      } else {
        console.log();
        console.log(dim('No source files found.'));
        if (extraOutput?.text) console.log(extraOutput.text);
        console.log();
      }
      return;
    }

    if (!jsonOutput) {
      process.stderr.write(
        `  ${dim(`Collecting symbols for ${files.length} file${files.length === 1 ? '' : 's'} ...`)}\n`
      );
    }

    const { structured, rendered } = await fetchDocumentSymbols(
      executeDirectTool,
      files,
      pageSize
    );

    if (jsonOutput) {
      console.log(
        JSON.stringify(
          { files, results: structured, ...(extraOutput?.structured ?? {}) },
          null,
          2
        )
      );
      return;
    }

    console.log();
    console.log(renderSymbols(rendered, kindFilter));
    if (extraOutput?.text) console.log(extraOutput.text);
    console.log();
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    if (jsonOutput) {
      console.log(
        JSON.stringify({
          success: false,
          error: `Octocode tool runtime failed: ${message}`,
        })
      );
    } else {
      console.error(
        `\n  ${c('red', '✗')} Octocode tool runtime failed: ${message}\n`
      );
    }
    process.exitCode = EXIT.TOOL;
  }
}
