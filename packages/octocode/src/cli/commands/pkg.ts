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

const MODE_VALUES = new Set(['lean', 'full']);

export const pkgCommand: CLICommand = {
  name: 'pkg',
  options: [
    { name: 'mode', hasValue: true },
    { name: 'page', hasValue: true },
    { name: 'json' },
  ],
  handler: async args => {
    const packageName = args.args[0] ?? '';
    const jsonOutput = getBool(args.options, 'json');
    const page = parsePage(getString(args.options, 'page'));
    const mode = getString(args.options, 'mode') || undefined;

    if (mode && !MODE_VALUES.has(mode)) {
      const error = 'Invalid --mode. Use lean or full.';
      if (jsonOutput) console.log(JSON.stringify({ success: false, error }));
      else console.error(`\n  ${c('red', '✗')} ${error}\n`);
      process.exitCode = EXIT.USAGE;
      return;
    }

    if (!packageName) {
      const error = 'Provide a package name or keyword query.';
      if (jsonOutput) {
        console.log(JSON.stringify({ success: false, error }));
      } else {
        console.error(`\n  ${c('red', '✗')} ${error}`);
        console.error(
          `\n  ${dim('Examples:')}\n` +
            `    pkg zod\n` +
            `    pkg @modelcontextprotocol/sdk\n` +
            `    pkg "react state management" --page 1\n`
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
            mode,
            mainResearchGoal: `Research npm package ${packageName}`,
            researchGoal:
              'Resolve package metadata, install guidance, and source repository',
            reasoning: 'CLI pkg command',
          },
        ],
      });

      printDirectToolResult(result, jsonOutput);
      if (!jsonOutput) {
        process.stderr.write(
          `  ${dim('Tip: Searches npm only. For Rust crates: crates.io · Python: pypi.org · Ruby: rubygems.org')}\n`
        );
      }
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
          `\n  ${c('red', '✗')} Octocode tool runtime failed: ${message}\n`
        );
      }
      process.exitCode = EXIT.TOOL;
    }
  },
};
