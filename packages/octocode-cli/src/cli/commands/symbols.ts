import path from 'node:path';
import { existsSync, statSync } from 'node:fs';
import type { CLICommand } from '../types.js';
import { getBool, getString } from '../options.js';
import { c, bold, dim } from '../../utils/colors.js';
import { EXIT } from '../exit-codes.js';
import { executeDirectTool } from '@octocodeai/octocode-tools-core/direct';
import { getDirectToolText } from './direct-tool-output.js';

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

type FileEntry = {
  readonly path?: unknown;
};

type SymbolEntry = {
  readonly name?: unknown;
  readonly kind?: unknown;
  readonly line?: unknown;
  readonly containerName?: unknown;
};

function parsePositiveInt(value: string, fallback: number): number {
  if (!value) return fallback;
  const parsed = Number.parseInt(value, 10);
  return Number.isInteger(parsed) && parsed > 0 ? parsed : fallback;
}

function parseExtensions(value: string): string[] {
  if (!value) {
    return [...DEFAULT_SOURCE_EXTENSIONS];
  }

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

function symbolsFromLspResult(value: unknown): Array<{
  readonly uri: string;
  readonly symbols: SymbolEntry[];
  readonly totalSymbols?: number;
}> {
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
  files: readonly {
    readonly uri: string;
    readonly symbols: readonly SymbolEntry[];
    readonly totalSymbols?: number;
  }[],
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

function printUsageError(message: string, jsonOutput: boolean): void {
  if (jsonOutput) {
    console.log(JSON.stringify({ success: false, error: message }));
    return;
  }

  console.error(`\n  ${c('red', 'x')} ${message}`);
  console.error(
    `\n  ${dim('Examples:')}\n` +
      `    octocode symbols src/index.ts\n` +
      `    octocode symbols src --ext ts,tsx --limit 10\n` +
      `    octocode symbols src/index.ts --kind function\n`
  );
}

async function discoverSourceFiles(
  executeDirectTool: DirectToolExecutor,
  dirPath: string,
  extensions: readonly string[],
  limit: number,
  depth: number
): Promise<string[]> {
  const result = await executeDirectTool('localFindFiles', {
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
        reasoning: 'CLI symbols command directory discovery',
      },
    ],
  });

  if (result.isError) {
    throw new Error(getDirectToolText(result));
  }

  return filePathsFromFindResult(result.structuredContent)
    .map(filePath =>
      path.isAbsolute(filePath) ? filePath : path.resolve(dirPath, filePath)
    )
    .slice(0, limit);
}

async function fetchDocumentSymbols(
  executeDirectTool: DirectToolExecutor,
  filePaths: readonly string[],
  pageSize: number
): Promise<{
  readonly structured: unknown[];
  readonly rendered: Array<{
    readonly uri: string;
    readonly symbols: SymbolEntry[];
    readonly totalSymbols?: number;
  }>;
}> {
  const structured: unknown[] = [];
  const rendered: Array<{
    readonly uri: string;
    readonly symbols: SymbolEntry[];
    readonly totalSymbols?: number;
  }> = [];

  for (const batch of chunk(filePaths, 5)) {
    const result = await executeDirectTool('lspGetSemantics', {
      queries: batch.map(filePath => ({
        uri: path.resolve(filePath),
        type: 'documentSymbols',
        itemsPerPage: pageSize,
        mainResearchGoal: 'Build semantic symbol outline',
        researchGoal: `List document symbols in ${filePath}`,
        reasoning: 'CLI symbols command',
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

export const symbolsCommand: CLICommand = {
  name: 'symbols',
  description: 'Show a semantic symbol outline for a local file or directory',
  usage:
    'octocode symbols <file|path> [--ext <list>] [--kind <kind>] [--limit <n>] [--depth <n>] [--page-size <n>] [--json]',
  options: [
    {
      name: 'ext',
      hasValue: true,
      description: 'Comma-separated source extensions for directory mode',
    },
    {
      name: 'kind',
      hasValue: true,
      description:
        'Filter rendered symbols by kind, e.g. function, class, method',
    },
    {
      name: 'limit',
      hasValue: true,
      description: 'Maximum files to inspect in directory mode (default: 10)',
    },
    {
      name: 'depth',
      hasValue: true,
      description: 'Directory discovery depth (default: 4)',
    },
    {
      name: 'page-size',
      hasValue: true,
      description: 'Symbols per file from LSP (default: 40)',
    },
    {
      name: 'json',
      description: 'Output raw JSON results',
    },
  ],
  handler: async args => {
    const target = args.args[0] ?? '';
    const jsonOutput = getBool(args.options, 'json');
    const kindFilter = getString(args.options, 'kind');
    const extensions = parseExtensions(getString(args.options, 'ext'));
    const limit = parsePositiveInt(getString(args.options, 'limit'), 10);
    const depth = parsePositiveInt(getString(args.options, 'depth'), 4);
    const pageSize = parsePositiveInt(getString(args.options, 'page-size'), 40);

    if (!target) {
      printUsageError('Provide a local file or directory path.', jsonOutput);
      process.exitCode = EXIT.USAGE;
      return;
    }

    const resolved = path.resolve(target);
    if (!existsSync(resolved)) {
      printUsageError(`Path not found: ${target}`, jsonOutput);
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
          console.log(JSON.stringify({ files: [], symbols: [] }, null, 2));
        } else {
          console.log();
          console.log(dim('No source files found.'));
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
            {
              files,
              results: structured,
            },
            null,
            2
          )
        );
        return;
      }

      console.log();
      console.log(renderSymbols(rendered, kindFilter));
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
          `\n  ${c('red', 'x')} Octocode tool runtime failed: ${message}\n`
        );
      }
      process.exitCode = EXIT.TOOL;
    }
  },
};
