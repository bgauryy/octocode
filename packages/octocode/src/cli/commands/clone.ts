import type { CLICommand } from '../types.js';
import { getBool, getString } from '../options.js';
import { resolveRef, isGithubRef, refLabel } from '../routing.js';
import { c, dim } from '../../utils/colors.js';
import { EXIT } from '../exit-codes.js';
import { executeDirectTool } from '@octocodeai/octocode-tools-core/direct';
import {
  getDirectToolText,
  markDirectToolFailure,
  printDirectToolResult,
} from './direct-tool-output.js';

type CloneStructuredContent = {
  results?: Array<{ data?: { localPath?: string } }>;
};

function cloneLocalPath(structuredContent: unknown): string | undefined {
  const content = structuredContent as CloneStructuredContent;
  return content.results?.[0]?.data?.localPath;
}

export const cloneCommand: CLICommand = {
  name: 'clone',
  options: [
    { name: 'branch', hasValue: true },
    { name: 'force-refresh' },
    { name: 'json' },
  ],
  handler: async args => {
    const target = args.args[0] ?? '';
    const jsonOutput = getBool(args.options, 'json');
    const branchOverride = getString(args.options, 'branch');

    const reportUsage = (message: string): void => {
      if (jsonOutput) {
        console.log(JSON.stringify({ success: false, error: message }));
      } else {
        console.error(`\n  ${c('red', '✗')} ${message}`);
        console.error(
          `\n  ${dim('Examples:')}\n` +
            `    clone vercel/next.js\n` +
            `    clone vercel/next.js/packages/next        ${dim('# sparse subtree')}\n` +
            `    clone vercel/next.js@canary/packages/next\n` +
            `    clone https://github.com/owner/repo/tree/main/src\n`
        );
      }
      process.exitCode = EXIT.USAGE;
    };

    if (!target) {
      reportUsage('Provide a GitHub ref: owner/repo[/path][@branch] or a URL.');
      return;
    }

    const ref = resolveRef(target, branchOverride || undefined);
    if (!isGithubRef(ref)) {
      reportUsage(
        `Not a GitHub ref: "${target}". Provide owner/repo[/path][@branch] or a github.com URL.`
      );
      return;
    }

    if (!jsonOutput) {
      process.stderr.write(`  ${dim(`Cloning ${refLabel(ref)} ...`)}\n`);
    }

    try {
      const result = await executeDirectTool('ghCloneRepo', {
        queries: [
          {
            owner: ref.owner,
            repo: ref.repo,
            branch: ref.branch,
            sparsePath: ref.subpath || undefined,
            forceRefresh: getBool(args.options, 'force-refresh') || undefined,
            mainResearchGoal: 'Clone a GitHub repository for local research',
            researchGoal: `Clone ${refLabel(ref)} for local search and LSP`,
            reasoning: 'CLI clone command',
          },
        ],
      });

      printDirectToolResult(result, jsonOutput);
      if (!jsonOutput && !result.isError) {
        const localPath = cloneLocalPath(result.structuredContent);
        if (localPath) {
          console.log(
            `  ${c('green', '→')} Local clone: ${c('cyan', localPath)}\n` +
              `    ${c('cyan', `search ${localPath} --tree`)}       ${dim('# map the tree')}\n` +
              `    ${c('cyan', `search <term> ${localPath}`)}        ${dim('# search locally')}\n` +
              `    ${c('cyan', `search ${localPath}/<file>`)}       ${dim('# read a file')}\n` +
              `    ${c('cyan', `search ${localPath}/<file> --symbols`)} ${dim('# semantic outline')}\n`
          );
        }
      }
      if (
        result.isError &&
        ref.subpath &&
        /is not a directory/i.test(getDirectToolText(result))
      ) {
        console.error(
          `\n  ${c('cyan', '→')} ${dim(`“${ref.subpath}” is a file, but clone checks out directories. For a single file use: search ${refLabel(ref)}  (or: cache fetch ${ref.owner}/${ref.repo} ${ref.subpath})`)}\n`
        );
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
