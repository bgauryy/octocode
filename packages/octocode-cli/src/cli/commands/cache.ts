import type { CLICommand, ParsedArgs } from '../types.js';
import { c, bold, dim } from '../../utils/colors.js';
import {
  clearSkillsCache,
  getSkillsCacheDir,
} from '../../utils/skills-fetch.js';
import path from 'node:path';
import { paths, getDirectorySizeBytes, formatBytes } from 'octocode-shared';
import { existsSync, rmSync } from 'node:fs';

export const cacheCommand: CLICommand = {
  name: 'cache',
  description: 'Inspect and clean Octocode cache and logs',
  usage:
    'octocode-cli cache [status|clean] [--repos] [--skills] [--logs] [--tools|--local|--lsp|--api] [--all]',
  options: [
    { name: 'repos', description: 'Target cloned repositories cache' },
    { name: 'skills', description: 'Target marketplace skills cache' },
    { name: 'logs', description: 'Target Octocode logs directory' },
    {
      name: 'all',
      short: 'a',
      description: 'Target repos + skills + logs (tool flags are advisory)',
    },
    {
      name: 'tools',
      description:
        'Target tool caches (local + lsp + api). In-memory caches clear on MCP restart.',
    },
    {
      name: 'local',
      description:
        'Target local tool cache. In-memory cache clears on MCP restart.',
    },
    {
      name: 'lsp',
      description:
        'Target LSP tool cache. In-memory cache clears on MCP restart.',
    },
    {
      name: 'api',
      description:
        'Target remote API tool cache. In-memory cache clears on MCP restart.',
    },
  ],
  handler: async (args: ParsedArgs) => {
    const subcommand = (args.args[0] || 'status').toLowerCase();
    const octocodeHome =
      paths.home ||
      process.env.OCTOCODE_HOME ||
      path.join(process.env.HOME || '', '.octocode');
    const reposDir = paths.repos || path.join(octocodeHome, 'repos');
    const logsDir = paths.logs || path.join(octocodeHome, 'logs');
    const skillsDir = getSkillsCacheDir();

    const hasReposFlag = Boolean(args.options['repos']);
    const hasSkillsFlag = Boolean(args.options['skills']);
    const hasLogsFlag = Boolean(args.options['logs']);
    const hasToolsFlag = Boolean(args.options['tools']);
    const hasLocalFlag = Boolean(args.options['local']);
    const hasLspFlag = Boolean(args.options['lsp']);
    const hasApiFlag = Boolean(args.options['api']);
    const hasAllFlag = Boolean(args.options['all'] || args.options['a']);

    const targetRepos = hasAllFlag || hasReposFlag;
    const targetSkills = hasAllFlag || hasSkillsFlag;
    const targetLogs = hasAllFlag || hasLogsFlag;
    const targetTools =
      hasToolsFlag || hasLocalFlag || hasLspFlag || hasApiFlag;

    if (subcommand === 'status') {
      const reposBytes = getDirectorySizeBytes(reposDir);
      const skillsBytes = getDirectorySizeBytes(skillsDir);
      const logsBytes = getDirectorySizeBytes(logsDir);
      const total = reposBytes + skillsBytes + logsBytes;

      console.log();
      console.log(`  ${bold('🧹 Octocode Cache Status')}`);
      console.log();
      console.log(`  ${dim('Home:')} ${octocodeHome}`);
      console.log();
      console.log(
        `  ${c('cyan', '•')} repos:  ${formatBytes(reposBytes)}  ${dim(`(${reposDir})`)}`
      );
      console.log(
        `  ${c('cyan', '•')} skills: ${formatBytes(skillsBytes)}  ${dim(`(${skillsDir})`)}`
      );
      console.log(
        `  ${c('cyan', '•')} logs:   ${formatBytes(logsBytes)}  ${dim(`(${logsDir})`)}`
      );
      console.log();
      console.log(`  ${bold('Total:')} ${formatBytes(total)}`);
      console.log();
      console.log(`  ${dim('Clean examples:')}`);
      console.log(`    ${c('yellow', 'octocode-cli cache clean --repos')}`);
      console.log(`    ${c('yellow', 'octocode-cli cache clean --tools')}`);
      console.log(`    ${c('yellow', 'octocode-cli cache clean --all')}`);
      console.log();
      return;
    }

    if (subcommand === 'clean') {
      if (!targetRepos && !targetSkills && !targetLogs && !targetTools) {
        console.log();
        console.log(
          `  ${c('red', 'X')} Missing clean target. Use --repos, --skills, --logs, --tools, or --all`
        );
        console.log(`  ${dim('Example:')} octocode-cli cache clean --all`);
        console.log();
        process.exitCode = 1;
        return;
      }

      let cleanedAnything = false;
      let freedBytes = 0;

      if (targetRepos && existsSync(reposDir)) {
        freedBytes += getDirectorySizeBytes(reposDir);
        rmSync(reposDir, { recursive: true, force: true });
        cleanedAnything = true;
      }

      if (targetSkills) {
        const before = getDirectorySizeBytes(skillsDir);
        clearSkillsCache();
        const after = getDirectorySizeBytes(skillsDir);
        if (before > 0 || after === 0) {
          freedBytes += Math.max(0, before - after);
          cleanedAnything = true;
        }
      }

      if (targetLogs && existsSync(logsDir)) {
        freedBytes += getDirectorySizeBytes(logsDir);
        rmSync(logsDir, { recursive: true, force: true });
        cleanedAnything = true;
      }

      if (targetTools) {
        const toolFlags = [
          hasToolsFlag ? '--tools' : '',
          hasLocalFlag ? '--local' : '',
          hasLspFlag ? '--lsp' : '',
          hasApiFlag ? '--api' : '',
        ]
          .filter(Boolean)
          .join(', ');
        console.log(
          `  ${c('yellow', 'ℹ')} ${toolFlags}: No disk caches to clean. Tool caches are in-memory and clear on MCP server restart.`
        );
      }

      console.log();
      if (cleanedAnything) {
        console.log(`  ${c('green', '✓')} Cache cleanup complete`);
        console.log(`  ${dim('Freed:')} ${formatBytes(freedBytes)}`);
      } else if (!targetTools) {
        console.log(`  ${c('yellow', '⚠')} Nothing to clean`);
      }
      console.log();
      return;
    }

    console.log();
    console.log(`  ${c('red', '✗')} Unknown cache subcommand: ${subcommand}`);
    console.log(
      `  ${dim('Usage:')} octocode-cli cache [status|clean] [--repos] [--skills] [--logs] [--tools|--local|--lsp|--api] [--all]`
    );
    console.log();
    process.exitCode = 1;
  },
};
