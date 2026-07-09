import type { CLICommand } from '../types.js';
import { join } from 'node:path';
import { getBool } from '../options.js';
import { c, dim } from '../../utils/colors.js';
import { EXIT } from '../exit-codes.js';
import { executeDirectTool } from '@octocodeai/octocode-tools-core/direct';
import { paths } from '@octocodeai/octocode-tools-core/paths';
import {
  markDirectToolFailure,
  printDirectToolResult,
} from './direct-tool-output.js';

const UNZIP_EXAMPLE_PATH = join(paths.unzip, 'app.zip-<timestamp>');

export const unzipCommand: CLICommand = {
  name: 'unzip',
  options: [{ name: 'json' }],
  handler: async args => {
    const file = args.args[0] ?? '';
    const jsonOutput = getBool(args.options, 'json');

    if (!file) {
      const error = 'Provide an archive path to unpack.';
      if (jsonOutput) {
        console.log(JSON.stringify({ success: false, error }));
      } else {
        console.error(`\n  ${c('red', '✗')} ${error}`);
        console.error(
          `\n  ${dim('Examples:')}\n` +
            `    unzip app.zip\n` +
            `    unzip release.tar.gz\n` +
            `    ${dim('# then work on the unpacked tree:')}\n` +
            `    search ${UNZIP_EXAMPLE_PATH} --tree\n` +
            `    search "apiKey" ${UNZIP_EXAMPLE_PATH}\n`
        );
      }
      process.exitCode = EXIT.USAGE;
      return;
    }

    if (!jsonOutput) {
      process.stderr.write(`  ${dim(`Unpacking ${file} ...`)}\n`);
    }

    try {
      const result = await executeDirectTool('localBinaryInspect', {
        queries: [
          {
            path: file,
            mode: 'unpack',
            mainResearchGoal: 'Unpack an archive for local research',
            researchGoal: `Extract ${file} to a local directory`,
            reasoning: 'CLI unzip command',
          },
        ],
      });

      printDirectToolResult(result, jsonOutput);

      if (!jsonOutput) {
        const sc = result.structuredContent as
          | { results?: Array<{ data?: { localPath?: string } }> }
          | undefined;
        const localPath = sc?.results?.[0]?.data?.localPath;
        if (localPath) {
          console.log(
            `  ${dim('Next — research it with the local tools:')}\n` +
              `    ${c('cyan', `search ${localPath} --tree`)}       ${dim('# map the tree')}\n` +
              `    ${c('cyan', `search <term> ${localPath}`)}        ${dim('# search code')}\n` +
              `    ${c('cyan', `search ${localPath}/<file>`)}       ${dim('# read a file')}\n`
          );
        }
      }

      markDirectToolFailure(result);
    } catch (caught) {
      const message = caught instanceof Error ? caught.message : String(caught);
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
