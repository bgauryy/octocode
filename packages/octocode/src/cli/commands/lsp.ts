import path from 'node:path';
import { existsSync, statSync } from 'node:fs';
import type { CLICommand } from '../types.js';
import { getBool, getString } from '../options.js';
import { c, dim } from '../../utils/colors.js';
import { EXIT } from '../exit-codes.js';
import { executeDirectTool } from '@octocodeai/octocode-tools-core/direct';
import {
  markDirectToolFailure,
  printDirectToolResult,
} from './direct-tool-output.js';
import {
  materializeRemoteForCli,
  type RemoteMaterialization,
  withMaterializationHints,
} from '../remote-local.js';

const LSP_TYPES = [
  'documentSymbols',
  'definition',
  'references',
  'callers',
  'callees',
  'callHierarchy',
  'hover',
  'typeDefinition',
  'implementation',
  'workspaceSymbol',
  'supertypes',
  'subtypes',
  'diagnostic',
] as const;

type LspType = (typeof LSP_TYPES)[number];

function parsePositiveInt(value: string): number | undefined {
  if (!value) return undefined;
  const parsed = Number.parseInt(value, 10);
  return Number.isInteger(parsed) && parsed > 0 ? parsed : undefined;
}

function isLspType(value: string): value is LspType {
  return LSP_TYPES.includes(value as LspType);
}

function printUsageError(message: string, jsonOutput: boolean): void {
  if (jsonOutput) {
    console.log(JSON.stringify({ success: false, error: message }));
    return;
  }

  console.error(`\n  ${c('red', '✗')} ${message}`);
  console.error(
    `\n  ${dim('Examples:')}\n` +
      `    lsp packages/octocode/src/index.ts --type references --symbol runCLI --line 10\n` +
      `    lsp packages/octocode/src/index.ts --type definition --symbol runCLI --line 10\n` +
      `    lsp packages/octocode/src/cli/index.ts --type hover --symbol runCLI --line 73\n` +
      `    lsp packages/octocode/src/cli/index.ts --type documentSymbols\n`
  );
}

type LspToolResult = {
  readonly isError?: boolean;
  readonly structuredContent?: {
    readonly results?: readonly LspQueryResult[];
    readonly hints?: readonly string[];
  };
};

type LspQueryResult = {
  readonly status?: string;
  readonly data?: {
    readonly type?: string;
    readonly resolvedSymbol?: {
      readonly name?: string;
      readonly foundAtLine?: number;
    };
    readonly payload?: LspPayload;
    readonly pagination?: {
      readonly hasMore?: boolean;
      readonly nextPage?: number;
    };
  };
};

type LspPayload = {
  readonly kind?: string;
  readonly category?: string;
  readonly reason?: string;
  readonly locations?: readonly LspLocation[];
  readonly markdown?: string;
  readonly text?: string;
  readonly calls?: readonly unknown[];
  readonly symbols?: readonly unknown[];
  readonly items?: readonly unknown[];
  readonly diagnostics?: readonly unknown[];
  readonly empty?: { readonly category?: string; readonly reason?: string };
};

type LspLocation =
  | string
  | {
      readonly uri?: string;
      readonly content?: string;
      readonly displayRange?: {
        readonly startLine?: number;
        readonly endLine?: number;
      };
      readonly isDefinition?: boolean;
    };

type SearchLineResult = {
  readonly isError?: boolean;
  readonly structuredContent?: {
    readonly results?: readonly {
      readonly data?: {
        readonly files?: readonly {
          readonly matches?: readonly { readonly line?: number }[];
        }[];
      };
    }[];
  };
};

function semanticExitCode(category: string | undefined): number | undefined {
  switch (category) {
    case 'symbolNotFound':
    case 'anchorFailed':
    case 'noLocations':
    case 'noReferences':
    case 'noHover':
    case 'noCalls':
      return EXIT.NOT_FOUND;
    case 'serverUnavailable':
    case 'unsupportedOperation':
      return EXIT.TOOL;
    default:
      return undefined;
  }
}

function markLspSemanticFailure(result: LspToolResult): void {
  const results = result.structuredContent?.results ?? [];
  for (const item of results) {
    const payload = item.data?.payload;
    const category =
      payload?.kind === 'empty' ? payload.category : payload?.empty?.category;
    const exitCode = semanticExitCode(category);
    if (exitCode !== undefined) {
      process.exitCode = exitCode;
      return;
    }
  }
}

function formatLocation(location: LspLocation): string {
  if (typeof location === 'string') return location;
  const range = location.displayRange
    ? `${location.displayRange.startLine ?? '?'}-${location.displayRange.endLine ?? '?'}`
    : '?';
  const uri = location.uri ?? '<unknown>';
  const marker = location.isDefinition ? ' definition' : '';
  const content = location.content ? ` | ${location.content.trim()}` : '';
  return `${uri}:${range}${marker}${content}`;
}

function formatCall(call: unknown): string {
  if (typeof call === 'string') return call;
  const c = call as {
    direction?: string;
    item?: { name?: string; kind?: string; uri?: string };
    ranges?: ReadonlyArray<{ line?: number }>;
  };
  const dir = c.direction ? `${c.direction} ` : '';
  const name = c.item?.name ?? '?';
  const kind = c.item?.kind ? ` (${c.item.kind})` : '';
  const at =
    c.item?.uri != null
      ? ` ${c.item.uri}${c.ranges?.[0]?.line != null ? `:${c.ranges[0].line}` : ''}`
      : '';
  return `${dir}${name}${kind}${at}`;
}

function formatSymbolRow(row: unknown): string {
  if (typeof row === 'string') return row;
  const s = row as { name?: string; kind?: string; line?: number };
  const kind = s.kind ? ` (${s.kind})` : '';
  const at = s.line != null ? ` L${s.line}` : '';
  return `${s.name ?? '?'}${kind}${at}`;
}

function formatDiagnosticRow(row: unknown): string {
  if (typeof row === 'string') return row;
  const d = row as {
    severity?: string | number;
    message?: string;
    source?: string;
    code?: string | number;
    range?: { start?: { line?: number } };
    displayRange?: { startLine?: number };
    line?: number;
  };
  const line =
    d.line ??
    d.displayRange?.startLine ??
    (d.range?.start?.line == null ? undefined : d.range.start.line + 1);
  const severity = d.severity == null ? 'diagnostic' : String(d.severity);
  const source = d.source ? ` ${d.source}` : '';
  const code = d.code == null ? '' : ` ${d.code}`;
  const at = line == null ? '' : ` L${line}`;
  return `${severity}${source}${code}${at}: ${d.message ?? JSON.stringify(row)}`;
}

function renderLspResult(result: LspToolResult): string {
  const lines: string[] = [];
  for (const item of result.structuredContent?.results ?? []) {
    const data = item.data;
    const payload = data?.payload;
    if (!data || !payload) continue;

    const symbol = data.resolvedSymbol?.name;
    const foundAtLine = data.resolvedSymbol?.foundAtLine;
    lines.push(
      `  ${c('cyan', data.type ?? 'lsp')}${symbol ? ` ${dim(symbol)}` : ''}${foundAtLine ? ` ${dim(`line ${foundAtLine}`)}` : ''}`
    );

    if (payload.kind === 'empty') {
      lines.push(
        `    ${c('red', payload.category ?? 'empty')}: ${payload.reason ?? 'No result'}`
      );
      continue;
    }

    if (payload.locations) {
      for (const location of payload.locations) {
        lines.push(`    ${formatLocation(location)}`);
      }
      continue;
    }

    if (payload.kind === 'hover') {
      lines.push(`    ${(payload.markdown ?? payload.text ?? '').trim()}`);
      continue;
    }

    if (payload.calls) {
      for (const call of payload.calls.slice(0, 20))
        lines.push(`    ${formatCall(call)}`);
      continue;
    }

    if (payload.items) {
      for (const item of payload.items.slice(0, 40))
        lines.push(`    ${formatSymbolRow(item)}`);
      continue;
    }

    if (payload.diagnostics) {
      for (const diagnostic of payload.diagnostics.slice(0, 40))
        lines.push(`    ${formatDiagnosticRow(diagnostic)}`);
      continue;
    }

    if (payload.symbols) {
      for (const symbolRow of payload.symbols.slice(0, 40))
        lines.push(`    ${formatSymbolRow(symbolRow)}`);
    }
  }

  const hints = result.structuredContent?.hints ?? [];
  if (hints.length > 0) lines.push('', ...hints.map(hint => `  ${dim(hint)}`));
  return lines.length > 0
    ? lines.join('\n')
    : JSON.stringify(result.structuredContent ?? result, null, 2);
}

function printLspToolResult(result: LspToolResult, jsonOutput: boolean): void {
  if (jsonOutput) {
    printDirectToolResult(result, true);
    return;
  }

  if (result.isError) {
    printDirectToolResult(result, false);
    return;
  }

  console.log();
  console.log(renderLspResult(result));
  console.log();
}

async function inferLineHint(uri: string, symbolName: string): Promise<number> {
  const result = (await executeDirectTool('localSearchCode', {
    queries: [
      {
        keywords: symbolName,
        path: uri,
        fixedString: true,
        maxFiles: 1,
        maxMatchesPerFile: 1,
        itemsPerPage: 1,
        mainResearchGoal: 'Find LSP line anchor',
        researchGoal: `Find ${symbolName} in ${uri}`,
        reasoning: 'CLI lsp auto-line fallback',
      },
    ],
  })) as SearchLineResult;

  if (result.isError) {
    throw new Error(`Could not infer --line for ${symbolName}.`);
  }

  const line =
    result.structuredContent?.results?.[0]?.data?.files?.[0]?.matches?.[0]
      ?.line;
  if (typeof line !== 'number') {
    throw new Error(
      `Could not infer --line for ${symbolName}; run grep or ls --symbols and pass --line explicitly.`
    );
  }
  return line;
}

export const lspCommand: CLICommand = {
  name: 'lsp',
  options: [
    { name: 'type', hasValue: true },
    { name: 'symbol', hasValue: true },
    { name: 'line', hasValue: true },
    { name: 'workspace-root', hasValue: true },
    { name: 'repo', hasValue: true },
    { name: 'branch', hasValue: true },
    { name: 'force-refresh' },
    { name: 'page', hasValue: true },
    { name: 'page-size', hasValue: true },
    { name: 'context-lines', hasValue: true },
    { name: 'depth', hasValue: true },
    { name: 'format', hasValue: true },
    { name: 'json' },
  ],
  handler: async args => {
    const target = args.args[0] ?? '';
    const jsonOutput = getBool(args.options, 'json');
    const rawType = getString(args.options, 'type');
    const symbolName = getString(args.options, 'symbol');
    const repoOption = getString(args.options, 'repo');
    let lineHint = parsePositiveInt(getString(args.options, 'line'));

    if (!target) {
      printUsageError('Provide a local source file path.', jsonOutput);
      process.exitCode = EXIT.USAGE;
      return;
    }

    if (!rawType || !isLspType(rawType)) {
      // Guard: if the first positional looks like a type name the user may have
      // written `lsp definition <file>` instead of `lsp <file> --type definition`.
      const positionalIsType = isLspType(target);
      const hint = positionalIsType
        ? ` (did you mean: lsp <file> --type ${target}?)`
        : '';
      printUsageError(
        `Provide --type with one of: ${LSP_TYPES.join(', ')}${hint}`,
        jsonOutput
      );
      process.exitCode = EXIT.USAGE;
      return;
    }

    const isDocumentSymbols = rawType === 'documentSymbols';
    const isDiagnostic = rawType === 'diagnostic';
    const isWorkspaceSymbol = rawType === 'workspaceSymbol';
    const needsSymbol = !isDocumentSymbols && !isDiagnostic;
    const needsLine = needsSymbol && !isWorkspaceSymbol;

    // Report the specific missing input (an invalid --line value is already
    // rejected centrally before we get here).
    if (needsLine && !symbolName && !lineHint) {
      printUsageError(
        '--symbol is required. For a file/dir outline, use: ls <file|dir> --symbols',
        jsonOutput
      );
      process.exitCode = EXIT.USAGE;
      return;
    }
    if (needsSymbol && !symbolName) {
      printUsageError(
        '--symbol <name> is required for this --type.',
        jsonOutput
      );
      process.exitCode = EXIT.USAGE;
      return;
    }
    let uri = path.resolve(target);
    let materializedWorkspaceRoot: string | undefined;
    let materializedRemote: RemoteMaterialization | undefined;
    if (repoOption) {
      try {
        materializedRemote = await materializeRemoteForCli({
          repoRef: repoOption,
          path: target,
          branch: getString(args.options, 'branch') || undefined,
          forceRefresh: getBool(args.options, 'force-refresh') || undefined,
          kind: 'file',
        });
        uri = materializedRemote.localPath;
        materializedWorkspaceRoot = materializedRemote.repoRoot;
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        printUsageError(message, jsonOutput);
        process.exitCode = EXIT.TOOL;
        return;
      }
    } else if (existsSync(uri) && statSync(uri).isDirectory()) {
      printUsageError('Provide a file path, not a directory.', jsonOutput);
      process.exitCode = EXIT.USAGE;
      return;
    }

    const workspaceRoot =
      getString(args.options, 'workspace-root') || materializedWorkspaceRoot;
    const page = parsePositiveInt(getString(args.options, 'page'));
    const itemsPerPage = parsePositiveInt(getString(args.options, 'page-size'));
    const contextLines = parsePositiveInt(
      getString(args.options, 'context-lines')
    );
    const depth = parsePositiveInt(getString(args.options, 'depth'));
    const format = getString(args.options, 'format');

    try {
      if (needsLine && !lineHint && symbolName) {
        lineHint = await inferLineHint(uri, symbolName);
        if (!jsonOutput) {
          process.stderr.write(
            `  ${dim(`Inferred line ${lineHint} for ${symbolName} ...`)}\n`
          );
        }
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      printUsageError(message, jsonOutput);
      process.exitCode = EXIT.USAGE;
      return;
    }

    if (!jsonOutput) {
      process.stderr.write(
        `  ${dim(`Running LSP ${rawType} on ${path.relative(process.cwd(), uri) || uri} ...`)}\n`
      );
    }

    try {
      const result = await executeDirectTool('lspGetSemantics', {
        queries: [
          {
            uri,
            type: rawType,
            ...(symbolName ? { symbolName } : {}),
            ...(lineHint ? { lineHint } : {}),
            ...(workspaceRoot
              ? { workspaceRoot: path.resolve(workspaceRoot) }
              : {}),
            ...(page ? { page } : {}),
            ...(itemsPerPage ? { itemsPerPage } : {}),
            ...(contextLines ? { contextLines } : {}),
            ...(depth ? { depth } : {}),
            ...(format ? { format } : {}),
            mainResearchGoal: `Run ${rawType} LSP research`,
            researchGoal: isDocumentSymbols
              ? `List document symbols in ${uri}`
              : isDiagnostic
                ? `List diagnostics in ${uri}`
                : isWorkspaceSymbol
                  ? `Find workspace symbols matching ${symbolName}`
                  : `Resolve ${rawType} for ${symbolName} near line ${lineHint}`,
            reasoning: 'CLI lsp command',
          },
        ],
      });

      const outputResult = materializedRemote
        ? withMaterializationHints(result, materializedRemote)
        : result;
      printLspToolResult(outputResult, jsonOutput);

      // For references, warn when the result looks incomplete due to monorepo
      // package-boundary scoping. The LSP server is started from the nearest
      // tsconfig.json, so it only sees files in that package — cross-package
      // callers are invisible. Emit a hint whenever references returns very few
      // results without an explicit --workspace-root override.
      if (!jsonOutput && rawType === 'references' && !workspaceRoot) {
        const items: readonly LspQueryResult[] =
          (outputResult as LspToolResult).structuredContent?.results ?? [];
        const totalRefs = items.reduce((sum: number, item: LspQueryResult) => {
          const total = (
            item.data?.payload as { totalReferences?: number } | undefined
          )?.totalReferences;
          return sum + (typeof total === 'number' ? total : 0);
        }, 0);
        if (totalRefs <= 1) {
          process.stderr.write(
            `\n  ${dim('Scope note:')} references is scoped to the anchor file's package. ` +
              `Pass ${dim('--workspace-root <monorepo-root>')} to search across packages.\n`
          );
        }
      }

      markLspSemanticFailure(outputResult);
      markDirectToolFailure(outputResult);
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
  },
};
