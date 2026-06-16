import type { CLICommand } from '../types.js';
import { getBool, getString } from '../options.js';
import { c, dim } from '../../utils/colors.js';
import { EXIT } from '../exit-codes.js';
import { executeDirectTool } from '@octocodeai/octocode-tools-core/direct';
import {
  markDirectToolFailure,
  printDirectToolResult,
} from './direct-tool-output.js';

function parsePage(value: string): number | undefined {
  if (!value) return undefined;
  const page = Number.parseInt(value, 10);
  return Number.isInteger(page) && page > 0 ? page : undefined;
}

export const pkgCommand: CLICommand = {
  name: 'pkg',
  description: 'Research an npm package and its source repository',
  usage: 'octocode pkg <package> [--page <n>] [--json]',
  options: [
    {
      name: 'page',
      hasValue: true,
      description: 'Result page for package keyword searches',
    },
    {
      name: 'json',
      description: 'Output raw JSON results',
    },
  ],
  handler: async args => {
    const packageName = args.args[0] ?? '';
    const jsonOutput = getBool(args.options, 'json');
    const page = parsePage(getString(args.options, 'page'));

    if (!packageName) {
      const error = 'Provide a package name.';
      if (jsonOutput) {
        console.log(JSON.stringify({ success: false, error }));
      } else {
        console.error(`\n  ${c('red', 'x')} ${error}`);
        console.error(
          `\n  ${dim('Examples:')}\n` +
            `    octocode pkg zod\n` +
            `    octocode pkg @modelcontextprotocol/sdk\n`
        );
      }
      process.exitCode = EXIT.USAGE;
      return;
    }

    if (!jsonOutput) {
      process.stderr.write(
        `  ${dim(`Researching package ${packageName} ...`)}\n`
      );
    }

    try {
      const result = await executeDirectTool('npmSearch', {
        queries: [
          {
            packageName,
            page,
            mainResearchGoal: `Research npm package ${packageName}`,
            researchGoal:
              'Resolve package metadata, install guidance, and source repository',
            reasoning: 'CLI pkg command',
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
