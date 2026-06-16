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

const LSP_TYPES = [
  'definition',
  'references',
  'callers',
  'callees',
  'callHierarchy',
  'hover',
  'documentSymbols',
  'typeDefinition',
  'implementation',
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

  console.error(`\n  ${c('red', 'x')} ${message}`);
  console.error(
    `\n  ${dim('Examples:')}\n` +
      `    octocode lsp src/index.ts --type documentSymbols\n` +
      `    octocode lsp src/index.ts --type references --symbol runCLI --line 42\n` +
      `    octocode lsp src/index.ts --type hover --symbol runCLI --line 42\n`
  );
}

export const lspCommand: CLICommand = {
  name: 'lsp',
  description: 'Run LSP semantic research for a local source file',
  usage:
    'octocode lsp <file> --type <type> [--symbol <name>] [--line <n>] [--workspace-root <path>] [--page <n>] [--page-size <n>] [--context-lines <n>] [--depth <n>] [--format structured|compact] [--json]',
  options: [
    {
      name: 'type',
      hasValue: true,
      description:
        'Semantic query: definition, references, callers, callees, callHierarchy, hover, documentSymbols, typeDefinition, implementation',
    },
    {
      name: 'symbol',
      hasValue: true,
      description: 'Symbol name; required unless type is documentSymbols',
    },
    {
      name: 'line',
      hasValue: true,
      description: 'Line hint; required unless type is documentSymbols',
    },
    {
      name: 'workspace-root',
      hasValue: true,
      description: 'Workspace root for the language server',
    },
    {
      name: 'page',
      hasValue: true,
      description: 'Result page for large LSP responses',
    },
    {
      name: 'page-size',
      hasValue: true,
      description: 'Results per page',
    },
    {
      name: 'context-lines',
      hasValue: true,
      description: 'Context lines around returned locations',
    },
    {
      name: 'depth',
      hasValue: true,
      description: 'Call hierarchy depth where supported',
    },
    {
      name: 'format',
      hasValue: true,
      description:
        'Output format passed to the LSP tool: structured or compact',
    },
    {
      name: 'json',
      description: 'Output raw JSON results',
    },
  ],
  handler: async args => {
    const target = args.args[0] ?? '';
    const jsonOutput = getBool(args.options, 'json');
    const rawType = getString(args.options, 'type');
    const symbolName = getString(args.options, 'symbol');
    const lineHint = parsePositiveInt(getString(args.options, 'line'));

    if (!target) {
      printUsageError('Provide a local source file path.', jsonOutput);
      process.exitCode = EXIT.USAGE;
      return;
    }

    if (!rawType || !isLspType(rawType)) {
      printUsageError(
        `Provide --type with one of: ${LSP_TYPES.join(', ')}`,
        jsonOutput
      );
      process.exitCode = EXIT.USAGE;
      return;
    }

    if (rawType !== 'documentSymbols' && (!symbolName || !lineHint)) {
      printUsageError(
        '--symbol and --line are required unless --type documentSymbols',
        jsonOutput
      );
      process.exitCode = EXIT.USAGE;
      return;
    }

    const uri = path.resolve(target);
    if (existsSync(uri) && statSync(uri).isDirectory()) {
      printUsageError('Provide a file path, not a directory.', jsonOutput);
      process.exitCode = EXIT.USAGE;
      return;
    }

    const workspaceRoot = getString(args.options, 'workspace-root');
    const page = parsePositiveInt(getString(args.options, 'page'));
    const itemsPerPage = parsePositiveInt(getString(args.options, 'page-size'));
    const contextLines = parsePositiveInt(
      getString(args.options, 'context-lines')
    );
    const depth = parsePositiveInt(getString(args.options, 'depth'));
    const format = getString(args.options, 'format');

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
            researchGoal:
              rawType === 'documentSymbols'
                ? `List document symbols in ${uri}`
                : `Resolve ${rawType} for ${symbolName} near line ${lineHint}`,
            reasoning: 'CLI lsp command',
          },
        ],
      });

      printDirectToolResult(result, jsonOutput);
      markDirectToolFailure(result);
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
