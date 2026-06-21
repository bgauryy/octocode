import type { CLICommand } from '../types.js';
import {
  getBool,
  getString,
  nonNegIntOption,
  posIntOption,
} from '../options.js';
import { resolveRef, isGithubRef, refLabel } from '../routing.js';
import { c, dim } from '../../utils/colors.js';
import { EXIT } from '../exit-codes.js';
import { executeDirectTool } from '@octocodeai/octocode-tools-core/direct';
import {
  getDirectToolText,
  markDirectToolFailure,
  printDirectToolResult,
} from './direct-tool-output.js';
import {
  materializeRemoteForCli,
  withMaterializationHints,
} from '../remote-local.js';

type MinifyMode = 'standard' | 'symbols' | 'none';

const VALID_MODES = new Set(['standard', 'symbols', 'none']);
const CONTENT_TYPES = new Set(['file', 'directory']);
const OPTION_NAMES = new Set([
  'help',
  'json',
  'compact',
  'no-color',
  'raw',
  'mode',
  'minify',
  'branch',
  'match-string',
  'match-regex',
  'match-case-sensitive',
  'start-line',
  'end-line',
  'context-lines',
  'char-offset',
  'char-length',
  'page',
  'page-size',
  'full-content',
  'force-refresh',
  'content-type',
  'repo',
]);

function reportUsage(message: string, jsonOutput: boolean): void {
  if (jsonOutput) {
    console.log(JSON.stringify({ success: false, error: message }));
  } else {
    console.error(`\n  ${c('red', '✗')} ${message}`);
    console.error(
      `\n  ${dim('Examples:')}\n` +
        `    cat src/utils.ts --match-string createClient --mode none\n` +
        `    cat bgauryy/octocode-mcp/README.md --match-string Octocode --mode none\n` +
        `    cat src/index.ts --start-line 40 --end-line 90 --mode none\n`
    );
  }
  process.exitCode = EXIT.USAGE;
}

function validateOptions(
  options: Record<string, string | boolean>,
  isGithub: boolean
): string | undefined {
  const unknown = Object.keys(options).find(
    option => !OPTION_NAMES.has(option)
  );
  if (unknown) return `Unknown cat option --${unknown}.`;

  if (getBool(options, 'raw') && getBool(options, 'json')) {
    return 'Use either --raw or --json, not both.';
  }

  // --minify is an alias for --mode (raw schema field is `minify`).
  const mode = getString(options, 'mode', 'minify') || 'standard';
  if (!VALID_MODES.has(mode)) {
    return 'Invalid --mode/--minify. Use none, standard, or symbols.';
  }

  const contentType = getString(options, 'content-type');
  if (contentType && !CONTENT_TYPES.has(contentType)) {
    return 'Invalid --content-type. Use file or directory.';
  }
  if (!isGithub && (getBool(options, 'force-refresh') || contentType)) {
    return '--force-refresh and --content-type are GitHub-only.';
  }

  for (const key of [
    'start-line',
    'end-line',
    'char-length',
    'page',
    'page-size',
  ]) {
    const value = getString(options, key);
    if (value && posIntOption(value) === undefined) {
      return `--${key} must be a positive integer.`;
    }
  }
  for (const key of ['context-lines', 'char-offset']) {
    const value = getString(options, key);
    if (value && nonNegIntOption(value) === undefined) {
      return `--${key} must be a non-negative integer.`;
    }
  }

  const startLine = posIntOption(getString(options, 'start-line'));
  const endLine = posIntOption(getString(options, 'end-line'));
  if (startLine && endLine && endLine < startLine) {
    return '--end-line must be greater than or equal to --start-line.';
  }

  return undefined;
}

function buildContentPaging(options: Record<string, string | boolean>): {
  charOffset?: number;
  charLength?: number;
} {
  const pageSize =
    posIntOption(getString(options, 'char-length')) ??
    posIntOption(getString(options, 'page-size'));
  const explicitOffset = nonNegIntOption(getString(options, 'char-offset'));
  const page = posIntOption(getString(options, 'page'));
  return {
    charLength: pageSize,
    charOffset:
      explicitOffset ??
      (pageSize && page && page > 1 ? (page - 1) * pageSize : undefined),
  };
}

function buildSharedQuery(
  path: string,
  options: Record<string, string | boolean>
): Record<string, unknown> {
  const paging = buildContentPaging(options);
  const mode =
    getString(options, 'mode', 'minify') ||
    (getBool(options, 'raw') ? 'none' : 'standard');
  return {
    path,
    fullContent: getBool(options, 'full-content') || undefined,
    matchString: getString(options, 'match-string') || undefined,
    matchStringIsRegex: getBool(options, 'match-regex') || undefined,
    matchStringCaseSensitive:
      getBool(options, 'match-case-sensitive') || undefined,
    startLine: posIntOption(getString(options, 'start-line')),
    endLine: posIntOption(getString(options, 'end-line')),
    contextLines: nonNegIntOption(getString(options, 'context-lines')),
    charOffset: paging.charOffset,
    charLength: paging.charLength,
    minify: mode as MinifyMode,
  };
}

type DirectToolResult = {
  readonly isError?: boolean;
  readonly structuredContent?: unknown;
};

function extractRawContent(result: DirectToolResult): string | undefined {
  const structured = result.structuredContent as
    | {
        readonly results?: readonly {
          readonly data?: { readonly content?: unknown };
          readonly files?: readonly { readonly content?: unknown }[];
        }[];
        readonly content?: unknown;
      }
    | undefined;
  const first = structured?.results?.[0];
  const content =
    first?.data?.content ?? first?.files?.[0]?.content ?? structured?.content;
  return typeof content === 'string' ? content : undefined;
}

type ContentPagination = {
  readonly hasMore?: boolean;
  readonly charOffset?: number;
  readonly charLength?: number;
  readonly currentPage?: number;
  readonly totalPages?: number;
};

/**
 * Extract the content pagination block. Local results carry it at
 * `data.pagination`; GitHub file results nest it under `data.files[0]`.
 */
function extractContentPagination(
  result: DirectToolResult
): ContentPagination | undefined {
  const data = (
    result.structuredContent as { results?: Array<{ data?: unknown }> }
  )?.results?.[0]?.data;
  const filePagination = (
    data as { files?: Array<{ pagination?: ContentPagination }> }
  )?.files?.[0]?.pagination;
  return (
    filePagination ?? (data as { pagination?: ContentPagination })?.pagination
  );
}

function printRawContent(result: DirectToolResult): boolean {
  const content = extractRawContent(result);
  if (content === undefined) {
    return false;
  }
  process.stdout.write(content);
  if (!content.endsWith('\n')) {
    process.stdout.write('\n');
  }
  return true;
}

export const catCommand: CLICommand = {
  name: 'cat',
  options: [
    { name: 'raw' },
    { name: 'mode', hasValue: true },
    { name: 'minify', hasValue: true },
    { name: 'branch', hasValue: true },
    { name: 'repo', hasValue: true },
    { name: 'match-string', hasValue: true },
    { name: 'match-regex' },
    { name: 'match-case-sensitive' },
    { name: 'start-line', hasValue: true },
    { name: 'end-line', hasValue: true },
    { name: 'context-lines', hasValue: true },
    { name: 'page-size', hasValue: true },
    { name: 'page', hasValue: true },
    { name: 'char-offset', hasValue: true },
    { name: 'char-length', hasValue: true },
    { name: 'full-content' },
    { name: 'content-type', hasValue: true },
    { name: 'force-refresh' },
    { name: 'json' },
  ],
  handler: async args => {
    const target = args.args[0] ?? '';
    const jsonOutput = getBool(args.options, 'json');
    const rawOutput = getBool(args.options, 'raw');
    const branchOverride = getString(args.options, 'branch');
    const repoOption = getString(args.options, 'repo');

    if (!target) {
      reportUsage('Provide a file path or GitHub reference.', jsonOutput);
      return;
    }

    const ref = resolveRef(target, branchOverride || undefined);
    const optionError = validateOptions(
      args.options,
      Boolean(repoOption) || isGithubRef(ref)
    );
    if (optionError) {
      reportUsage(optionError, jsonOutput);
      return;
    }

    if (!jsonOutput && !rawOutput) {
      process.stderr.write(`  ${dim(`Fetching ${refLabel(ref)} ...`)}\n`);
    }

    try {
      const materialized = repoOption
        ? await materializeRemoteForCli({
            repoRef: repoOption,
            path: target,
            branch: branchOverride || undefined,
            forceRefresh: getBool(args.options, 'force-refresh') || undefined,
            kind: 'file',
          })
        : undefined;
      const toolName = materialized
        ? 'localGetFileContent'
        : isGithubRef(ref)
          ? 'ghGetFileContent'
          : 'localGetFileContent';
      const query = materialized
        ? {
            ...buildSharedQuery(materialized.localPath, args.options),
            mainResearchGoal: 'Fetch materialized GitHub file content locally',
            researchGoal: `Read ${materialized.localPath}`,
            reasoning: 'CLI cat --repo command',
          }
        : isGithubRef(ref)
          ? {
              ...buildSharedQuery(ref.subpath || '.', args.options),
              owner: ref.owner,
              repo: ref.repo,
              branch: ref.branch,
              forceRefresh: getBool(args.options, 'force-refresh') || undefined,
              type: getString(args.options, 'content-type') || undefined,
              mainResearchGoal: 'Fetch GitHub file content',
              researchGoal: `Read ${refLabel(ref)}`,
              reasoning: 'CLI cat command',
            }
          : {
              ...buildSharedQuery(ref.path, args.options),
              mainResearchGoal: 'Fetch local file content',
              researchGoal: `Read ${ref.path}`,
              reasoning: 'CLI cat command',
            };

      const result = await executeDirectTool(toolName, { queries: [query] });

      if (
        result.isError &&
        !materialized &&
        isGithubRef(ref) &&
        /path is a directory/i.test(getDirectToolText(result))
      ) {
        if (!jsonOutput && !rawOutput) {
          process.stderr.write(
            `  ${dim('Path is a directory — switching to ls view ...')}\n`
          );
        }
        const treeResult = await executeDirectTool('ghViewRepoStructure', {
          queries: [
            {
              owner: ref.owner,
              repo: ref.repo,
              path: ref.subpath || '',
              branch: ref.branch,
              maxDepth: 2,
              mainResearchGoal: 'View directory structure',
              researchGoal: `Get GitHub directory tree for ${refLabel(ref)}`,
              reasoning: 'Auto-rerouted from get command (path is a directory)',
            },
          ],
        });
        printDirectToolResult(treeResult, jsonOutput);
        markDirectToolFailure(treeResult);
        return;
      }

      if (!result.isError && rawOutput) {
        if (!printRawContent(result)) {
          console.error(
            `\n  ${c('red', '✗')} No content returned for raw output.\n`
          );
          process.exitCode = EXIT.TOOL;
          return;
        }
        return;
      }

      const outputResult =
        materialized && !rawOutput
          ? withMaterializationHints(result, materialized)
          : result;
      printDirectToolResult(outputResult, jsonOutput);
      if (!jsonOutput && !result.isError) {
        const pagination = extractContentPagination(outputResult);
        if (pagination?.hasMore) {
          const nextOffset =
            (pagination.charOffset ?? 0) + (pagination.charLength ?? 0);
          const page = pagination.currentPage ? pagination.currentPage + 1 : 2;
          const totalPages = pagination.totalPages
            ? `/${pagination.totalPages}`
            : '';
          console.log(
            `  ${dim(`More content — use --char-offset ${nextOffset} (page ${page}${totalPages})`)}\n`
          );
        }
      }
      markDirectToolFailure(outputResult);
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
