import type { CLICommand, ParsedArgs } from '../types.js';
import { c, bold, dim } from '../../utils/colors.js';
import { Spinner } from '../../utils/spinner.js';
import {
  quickSync,
  readAllClientConfigs,
  analyzeSyncState,
  getClientDisplayName,
} from '../../features/sync.js';
import type { MCPClient } from '../../types/index.js';

export const syncCommand: CLICommand = {
  name: 'sync',
  aliases: ['sy'],
  description: 'Sync MCP configurations across all IDE clients',
  usage: 'octocode sync [--force] [--dry-run] [--status]',
  options: [
    {
      name: 'force',
      short: 'f',
      description: 'Auto-resolve conflicts (use first variant)',
    },
    {
      name: 'dry-run',
      short: 'n',
      description: 'Show what would be synced without making changes',
    },
    {
      name: 'status',
      short: 's',
      description: 'Show sync status without syncing',
    },
    {
      name: 'json',
      short: 'j',
      description: 'Output as JSON',
    },
  ],
  handler: async (args: ParsedArgs) => {
    const subcommand = args.args[0];
    const isPlan = subcommand === 'plan';
    const force = Boolean(args.options['force'] || args.options['f']);
    const dryRun =
      isPlan || Boolean(args.options['dry-run'] || args.options['n']);
    const statusOnly = Boolean(args.options['status'] || args.options['s']);
    const jsonOutput = Boolean(args.options['json'] || args.options['j']);

    if (statusOnly) {
      const spinner = jsonOutput
        ? null
        : new Spinner('Scanning configurations...').start();
      const snapshots = readAllClientConfigs();
      const analysis = analyzeSyncState(snapshots);
      spinner?.stop();

      if (jsonOutput) {
        console.log(
          JSON.stringify({
            clients: analysis.clients.map(s => ({
              client: s.client,
              name: getClientDisplayName(s.client),
              exists: s.exists,
              mcpCount: s.mcpCount,
            })),
            summary: analysis.summary,
          })
        );
        return;
      }

      console.log();
      console.log(`  ${bold('🔄 MCP Sync Status')}`);
      console.log();

      console.log(
        `  ${bold('Clients:')} ${analysis.summary.clientsWithConfig} with MCP configs`
      );
      console.log();

      for (const snapshot of analysis.clients) {
        const name = getClientDisplayName(snapshot.client);
        const icon = snapshot.exists ? c('green', '●') : c('dim', '○');
        const mcpInfo = snapshot.exists
          ? `${snapshot.mcpCount} MCPs`
          : dim('no config');
        console.log(`    ${icon} ${name}: ${mcpInfo}`);
      }

      console.log();
      console.log(`  ${bold('MCPs:')}`);
      console.log(
        `    ${c('cyan', '•')} ${analysis.summary.totalUniqueMCPs} unique MCPs`
      );

      if (analysis.summary.consistentMCPs > 0) {
        console.log(
          `    ${c('green', '✓')} ${analysis.summary.consistentMCPs} fully synced`
        );
      }
      if (analysis.summary.needsSyncCount > 0) {
        console.log(
          `    ${c('yellow', '○')} ${analysis.summary.needsSyncCount} can be auto-synced`
        );
      }
      if (analysis.summary.conflictCount > 0) {
        console.log(
          `    ${c('red', '!')} ${analysis.summary.conflictCount} have conflicts`
        );
        for (const diff of analysis.conflicts) {
          const clients = Array.from(diff.variants.keys())
            .map(getClientDisplayName)
            .join(' vs ');
          console.log(
            `       ${c('red', '•')} ${diff.mcpId}  ${dim(`(${clients})`)}`
          );
        }
      }

      console.log();

      if (
        analysis.summary.needsSyncCount > 0 ||
        analysis.summary.conflictCount > 0
      ) {
        console.log(
          `  ${dim('Run')} ${c('cyan', 'octocode sync')} ${dim('to synchronize.')}`
        );
        if (analysis.summary.conflictCount > 0) {
          console.log(
            `  ${dim('Use')} ${c('cyan', '--force')} ${dim('to auto-resolve conflicts.')}`
          );
        }
        console.log();
      }

      return;
    }

    if (dryRun) {
      const planSpinner = jsonOutput
        ? null
        : new Spinner('Analyzing configurations...').start();
      const snapshots = readAllClientConfigs();
      const analysis = analyzeSyncState(snapshots);
      planSpinner?.stop();

      const operations = [
        ...analysis.needsSync.map(diff => ({
          type: 'add' as const,
          mcpId: diff.mcpId,
          presentIn: diff.presentIn as MCPClient[],
          missingIn: diff.missingIn as MCPClient[],
          hasConflict: false,
        })),
        ...analysis.conflicts.map(diff => ({
          type: 'conflict' as const,
          mcpId: diff.mcpId,
          presentIn: diff.presentIn as MCPClient[],
          missingIn: diff.missingIn as MCPClient[],
          hasConflict: true,
        })),
        ...analysis.fullyConsistent.map(diff => ({
          type: 'ok' as const,
          mcpId: diff.mcpId,
          presentIn: diff.presentIn as MCPClient[],
          missingIn: [] as MCPClient[],
          hasConflict: false,
        })),
      ];

      if (jsonOutput) {
        console.log(
          JSON.stringify({
            dryRun: true,
            operations,
            summary: analysis.summary,
          })
        );
        return;
      }

      console.log();
      console.log(
        `  ${bold('🔄 Sync Plan')}  ${dim(`(${operations.length} MCPs across ${analysis.summary.clientsWithConfig} clients)`)}`
      );
      console.log();
      const actionable = operations.filter(o => o.type !== 'ok');
      for (const op of actionable) {
        const icon = op.type === 'conflict' ? c('red', '!') : c('yellow', '○');
        const action =
          op.type === 'conflict'
            ? `${c('red', 'conflict')} — needs --force to resolve`
            : `add to ${op.missingIn.join(', ')}`;
        console.log(`  ${icon} ${bold(op.mcpId)}  ${dim(action)}`);
      }
      if (actionable.length === 0) {
        console.log(`  ${c('green', '✓')} All MCPs are in sync.`);
      }
      console.log();
      if (analysis.needsSync.length > 0 || analysis.conflicts.length > 0) {
        console.log(
          `  ${dim('Run')} ${c('cyan', 'octocode sync')} ${dim('to apply.')}`
        );
        if (analysis.conflicts.length > 0) {
          console.log(
            `  ${dim('Use')} ${c('cyan', '--force')} ${dim('to auto-resolve conflicts.')}`
          );
        }
        console.log();
      }
      return;
    }

    const spinner = jsonOutput
      ? null
      : new Spinner('Analyzing configurations...').start();

    const result = await quickSync({ force, dryRun: false });

    if (jsonOutput) {
      console.log(
        JSON.stringify({
          success: result.success,
          syncPerformed: result.syncPerformed,
          dryRun: false,
          message: result.message,
        })
      );
      if (!result.success) process.exitCode = 1;
      return;
    }

    console.log();
    console.log(`  ${bold('🔄 MCP Sync')}`);
    console.log();

    if (result.syncPerformed) {
      if (result.success) {
        spinner?.succeed(result.message);
        console.log();
        console.log(`  ${bold('Next:')} Restart your IDEs to apply changes.`);
      } else {
        spinner?.fail(result.message);
        process.exitCode = 1;
      }
    } else {
      spinner?.stop();
      if (result.success) {
        console.log(`  ${c('green', '✓')} ${result.message}`);
      } else {
        console.log(`  ${c('yellow', '⚠')} ${result.message}`);
        if (!force && result.message.includes('conflict')) {
          console.log();
          console.log(`  ${dim('Options:')}`);
          console.log(
            `    ${c('cyan', '•')} Run ${c('cyan', 'octocode')} for interactive mode`
          );
          console.log(
            `    ${c('cyan', '•')} Use ${c('cyan', '--force')} to auto-resolve`
          );
        }
        process.exitCode = 1;
      }
    }

    console.log();
  },
};
