import type { CLICommand } from '../types.js';
import { getBool, getString } from '../options.js';
import { c, dim } from '../../utils/colors.js';
import { EXIT } from '../exit-codes.js';
import { executeDirectTool } from '@octocodeai/octocode-tools-core/direct';
import {
  markDirectToolFailure,
  printDirectToolResult,
} from './direct-tool-output.js';

const SORT_VALUES = new Set([
  'stars',
  'forks',
  'help-wanted-issues',
  'updated',
  'best-match',
]);

const MATCH_VALUES = new Set(['name', 'description', 'readme']);
const VISIBILITY_VALUES = new Set(['public', 'private']);
const OPTION_NAMES = new Set([
  'help',
  'json',
  'compact',
  'no-color',
  'topic',
  'language',
  'owner',
  'stars',
  'forks',
  'good-first-issues',
  'license',
  'created',
  'updated',
  'size',
  'match',
  'sort',
  'archived',
  'visibility',
  'limit',
  'page',
  'verbose',
]);

type RepoSearchQuery = {
  keywordsToSearch?: string[];
  topicsToSearch?: string[];
  language?: string;
  owner?: string;
  stars?: string;
  size?: string;
  created?: string;
  updated?: string;
  match?: string[];
  sort?: string;
  limit?: number;
  page?: number;
  archived?: boolean;
  visibility?: string;
  forks?: string;
  license?: string;
  goodFirstIssues?: string;
  verbose?: boolean;
  mainResearchGoal: string;
  researchGoal: string;
  reasoning: string;
};

function parsePositiveInt(value: string): number | undefined {
  if (!value) return undefined;
  const parsed = Number.parseInt(value, 10);
  return Number.isInteger(parsed) && parsed > 0 ? parsed : undefined;
}

function parseList(value: string): string[] | undefined {
  const items = value
    .split(',')
    .map(item => item.trim())
    .filter(Boolean);
  return items.length > 0 ? items : undefined;
}

function parseBooleanOption(
  value: string | boolean | undefined
): boolean | undefined | 'invalid' {
  if (value === undefined) return undefined;
  if (typeof value === 'boolean') return value;

  const normalized = value.trim().toLowerCase();
  if (['true', '1', 'yes', 'y'].includes(normalized)) return true;
  if (['false', '0', 'no', 'n'].includes(normalized)) return false;
  return 'invalid';
}

function reportUsageError(message: string, jsonOutput: boolean): void {
  if (jsonOutput) {
    console.log(JSON.stringify({ success: false, error: message }));
  } else {
    console.error(`\n  ${c('red', 'x')} ${message}`);
    console.error(
      `\n  ${dim('Examples:')}\n` +
        `    octocode repo react state --language TypeScript --stars '>1000'\n` +
        `    octocode repo --topic mcp,agents --sort stars --limit 10\n` +
        `    octocode repo --owner vercel --language TypeScript --verbose\n`
    );
  }
  process.exitCode = EXIT.USAGE;
}

function hasResearchFilter(query: RepoSearchQuery): boolean {
  return Boolean(
    query.keywordsToSearch?.length ||
    query.topicsToSearch?.length ||
    query.language ||
    query.owner ||
    query.stars ||
    query.size ||
    query.created ||
    query.updated ||
    query.archived !== undefined ||
    query.visibility ||
    query.forks ||
    query.license ||
    query.goodFirstIssues
  );
}

function buildQuery(
  args: Parameters<CLICommand['handler']>[0]
): { query: RepoSearchQuery } | { error: string } {
  const unknownOption = Object.keys(args.options).find(
    option => !OPTION_NAMES.has(option)
  );
  if (unknownOption) {
    return {
      error:
        unknownOption === 'topics'
          ? 'Unknown repo option --topics. Use --topic <list>.'
          : `Unknown repo option --${unknownOption}.`,
    };
  }

  const keywords = args.args.map(arg => arg.trim()).filter(Boolean);
  const topics = parseList(getString(args.options, 'topic'));
  const match = parseList(getString(args.options, 'match'));
  const sort = getString(args.options, 'sort');
  const visibility = getString(args.options, 'visibility');
  const limitValue = getString(args.options, 'limit');
  const pageValue = getString(args.options, 'page');
  const archived = parseBooleanOption(args.options.archived);

  if (sort && !SORT_VALUES.has(sort)) {
    return {
      error:
        'Invalid --sort. Use stars, forks, help-wanted-issues, updated, or best-match.',
    };
  }

  if (visibility && !VISIBILITY_VALUES.has(visibility)) {
    return { error: 'Invalid --visibility. Use public or private.' };
  }

  if (match?.some(value => !MATCH_VALUES.has(value))) {
    return { error: 'Invalid --match. Use name, description, and/or readme.' };
  }

  if (archived === 'invalid') {
    return { error: 'Invalid --archived. Use true or false.' };
  }

  const limit = parsePositiveInt(limitValue);
  if (limitValue && !limit) {
    return { error: 'Invalid --limit. Use a positive integer.' };
  }

  const page = parsePositiveInt(pageValue);
  if (pageValue && !page) {
    return { error: 'Invalid --page. Use a positive integer.' };
  }

  const query: RepoSearchQuery = {
    ...(keywords.length > 0 && { keywordsToSearch: keywords }),
    ...(topics && { topicsToSearch: topics }),
    ...(getString(args.options, 'language') && {
      language: getString(args.options, 'language'),
    }),
    ...(getString(args.options, 'owner') && {
      owner: getString(args.options, 'owner'),
    }),
    ...(getString(args.options, 'stars') && {
      stars: getString(args.options, 'stars'),
    }),
    ...(getString(args.options, 'size') && {
      size: getString(args.options, 'size'),
    }),
    ...(getString(args.options, 'created') && {
      created: getString(args.options, 'created'),
    }),
    ...(getString(args.options, 'updated') && {
      updated: getString(args.options, 'updated'),
    }),
    ...(match && { match }),
    ...(sort && { sort }),
    ...(limit && { limit }),
    ...(page && { page }),
    ...(archived !== undefined && { archived }),
    ...(visibility && { visibility }),
    ...(getString(args.options, 'forks') && {
      forks: getString(args.options, 'forks'),
    }),
    ...(getString(args.options, 'license') && {
      license: getString(args.options, 'license'),
    }),
    ...(getString(args.options, 'good-first-issues') && {
      goodFirstIssues: getString(args.options, 'good-first-issues'),
    }),
    ...(getBool(args.options, 'verbose') && { verbose: true }),
    mainResearchGoal:
      keywords.length > 0
        ? `Search GitHub repositories for ${keywords.join(' ')}`
        : 'Search GitHub repositories',
    researchGoal:
      'Discover relevant repositories to inspect with tree, search, get, or pr commands',
    reasoning: 'CLI repo command',
  };

  if (!hasResearchFilter(query)) {
    return {
      error: 'Provide repo keywords or at least one repo search filter.',
    };
  }

  return { query };
}

export const repoCommand: CLICommand = {
  name: 'repo',
  description: 'Search GitHub repositories with research-oriented filters',
  usage:
    'octocode repo <keywords...> [--topic <list>] [--language <lang>] [--owner <owner>] [--stars <range>] [--sort stars|forks|help-wanted-issues|updated|best-match] [--verbose] [--json]',
  options: [
    { name: 'topic', hasValue: true, description: 'Comma-separated topics' },
    { name: 'language', hasValue: true, description: 'Language filter' },
    { name: 'owner', hasValue: true, description: 'Owner or organization' },
    { name: 'stars', hasValue: true, description: 'Stars range, e.g. >1000' },
    { name: 'forks', hasValue: true, description: 'Forks range' },
    {
      name: 'good-first-issues',
      hasValue: true,
      description: 'Good-first-issues range',
    },
    { name: 'license', hasValue: true, description: 'SPDX license key' },
    { name: 'created', hasValue: true, description: 'Created date range' },
    { name: 'updated', hasValue: true, description: 'Pushed date range' },
    { name: 'size', hasValue: true, description: 'Repository size range' },
    {
      name: 'match',
      hasValue: true,
      description: 'Comma-separated scopes: name,description,readme',
    },
    {
      name: 'sort',
      hasValue: true,
      description:
        'Sort: stars, forks, help-wanted-issues, updated, best-match',
    },
    {
      name: 'archived',
      hasValue: true,
      description: 'Include only archived repos when true',
    },
    {
      name: 'visibility',
      hasValue: true,
      description: 'Visibility: public or private',
    },
    { name: 'limit', hasValue: true, description: 'Max repositories to show' },
    { name: 'page', hasValue: true, description: 'Result page' },
    { name: 'verbose', description: 'Return structured repository objects' },
    { name: 'json', description: 'Output raw JSON results' },
  ],
  handler: async args => {
    const jsonOutput = getBool(args.options, 'json');
    const built = buildQuery(args);

    if ('error' in built) {
      reportUsageError(built.error, jsonOutput);
      return;
    }

    if (!jsonOutput) {
      const label =
        built.query.keywordsToSearch?.join(' ') ??
        built.query.topicsToSearch?.join(',') ??
        built.query.owner ??
        'repositories';
      process.stderr.write(
        `  ${dim(`Searching GitHub repos for ${label} ...`)}\n`
      );
    }

    try {
      const result = await executeDirectTool('ghSearchRepos', {
        queries: [built.query],
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
