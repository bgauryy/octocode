import type { ParsedArgs } from '../../types.js';
import { getBool, getString } from '../../options.js';
import { dim } from '../../../utils/colors.js';
import { EXIT } from '../../exit-codes.js';
import { printCliError } from '../../cli-error.js';
import { resolveRef, isGithubRef, cloneCommandFor } from '../../routing.js';
import { outlineSymbols } from '../symbol-outline.js';
import {
  formatMaterializationHints,
  materializeRemoteForCli,
  withMaterializationHints,
} from '../../remote-local.js';

export async function tryHandleSymbolOutline(
  args: ParsedArgs
): Promise<boolean> {
  const { options } = args;
  if (!getBool(options, 'symbols')) return false;

  const positionals = args.args.filter(a => !a.startsWith('-'));
  const target = positionals[0] ?? '';
  const repoOption = getString(options, 'repo') || undefined;
  const branchOverride = getString(options, 'branch') || undefined;

  if (repoOption) {
    try {
      const materialized = await materializeRemoteForCli({
        repoRef: repoOption,
        path: target || undefined,
        branch: branchOverride,
        forceRefresh: getBool(options, 'force-refresh') || undefined,
        kind: target ? 'file' : 'repo',
      });
      if (!getBool(options, 'json')) {
        process.stderr.write(
          `  ${dim(`Outlining ${materialized.localPath} ...`)}\n`
        );
      }
      await outlineSymbols(materialized.localPath, options, {
        structured: withMaterializationHints(
          { structuredContent: {} },
          materialized
        ).structuredContent as Record<string, unknown>,
        text: formatMaterializationHints(materialized),
      });
    } catch (error) {
      printCliError(
        `Remote materialization failed: ${
          error instanceof Error ? error.message : String(error)
        }`
      );
      process.exitCode = EXIT.TOOL;
    }
    return true;
  }

  if (!target) {
    printCliError('search --symbols needs a local path or --repo target.');
    process.exitCode = EXIT.USAGE;
    return true;
  }

  const ref = resolveRef(target, branchOverride);
  if (isGithubRef(ref)) {
    printCliError(
      '--symbols is local-only — an LSP outline cannot run on GitHub. ' +
        `Clone first: \`${cloneCommandFor(ref)}\`, then \`search <local-path> --symbols\`, ` +
        'or use `search <path> --repo <owner/repo> --symbols`.'
    );
    process.exitCode = EXIT.USAGE;
    return true;
  }

  await outlineSymbols(target, options);
  return true;
}
