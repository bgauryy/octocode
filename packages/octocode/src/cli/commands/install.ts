import type { CLICommand, ParsedArgs } from '../types.js';
import type { InstallMethod, MCPClient } from '../../types/index.js';
import { c, bold, dim } from '../../utils/colors.js';
import {
  installOctocodeForClient,
  getInstallPreviewForClient,
} from '../../features/install.js';
import { checkNodeInPath, checkNpmInPath } from '../../features/node-check.js';
import { INSTALL_METHOD_INFO } from '../../ui/constants.js';
import { Spinner } from '../../utils/spinner.js';
import {
  formatSupportedMCPClients,
  getIDEDisplayName,
  normalizeMCPClient,
  printNodeDoctorHintCLI,
} from './shared.js';
import { DETECTABLE_MCP_CLIENTS } from '../../utils/mcp-paths.js';
import { existsSync, accessSync, constants } from 'node:fs';
import { readMCPConfig } from '../../utils/mcp-io.js';
import { EXIT } from '../exit-codes.js';
import path from 'node:path';
import { runRollback } from './install/rollback.js';

const SUPPORTED_INSTALL_CLIENTS = DETECTABLE_MCP_CLIENTS;
const SUPPORTED_INSTALL_CLIENTS_TEXT = formatSupportedMCPClients();

export const installCommand: CLICommand = {
  name: 'install',
  options: [
    { name: 'ide', hasValue: true },
    { name: 'method', hasValue: true, default: 'npx' },
    { name: 'force' },
    { name: 'check' },
    { name: 'rollback' },
    { name: 'backup-path', hasValue: true },
    { name: 'json' },
  ],
  handler: async (args: ParsedArgs) => {
    const rawIde = args.options['ide'];
    const methodOpt = args.options['method'];
    const method = (typeof methodOpt === 'string' ? methodOpt : 'npx') as
      InstallMethod | string;
    const force = Boolean(args.options['force']);
    const checkOnly = Boolean(args.options['check']);
    const rollback = Boolean(args.options['rollback']);
    const rawBackupPath = args.options['backup-path'];
    const jsonOutput = Boolean(args.options['json']);

    if (typeof rawIde !== 'string' || rawIde.trim().length === 0) {
      if (!process.stdout.isTTY || jsonOutput) {
        if (jsonOutput) {
          console.log(
            JSON.stringify({
              success: false,
              ide: null,
              configPath: null,
              method,
              error: `Missing required option: --ide. Supported: ${SUPPORTED_INSTALL_CLIENTS_TEXT}`,
            })
          );
        } else {
          console.log();
          console.log(`  Missing required option: --ide`);
          console.log(`  Supported: ${SUPPORTED_INSTALL_CLIENTS_TEXT}`);
          console.log(`  Example: install --ide cursor`);
          console.log();
        }
        process.exitCode = EXIT.USAGE;
        return;
      }
      const { runInteractiveMode } = await import('../../interactive.js');
      await runInteractiveMode();
      return;
    }

    const client = normalizeMCPClient(rawIde);

    if (rollback) {
      runRollback({
        client: client as MCPClient,
        rawIde,
        rawBackupPath,
        jsonOutput,
      });
      return;
    }

    if (method === 'npx') {
      const nodeCheck = checkNodeInPath();
      const npmCheck = checkNpmInPath();

      if (!nodeCheck.installed) {
        if (jsonOutput) {
          console.log(
            JSON.stringify({
              success: false,
              ide: rawIde,
              configPath: null,
              method,
              error: 'Node.js is not found in PATH',
            })
          );
          process.exitCode = EXIT.GENERAL;
          return;
        }
        console.log();
        console.log(
          `  ${c('red', '✗')} Node.js is ${c('red', 'not found in PATH')}`
        );
        console.log(
          `  ${dim('Node.js is required for npx installation method.')}`
        );
        console.log();
        printNodeDoctorHintCLI();
        process.exitCode = EXIT.GENERAL;
        return;
      }

      if (!npmCheck.installed) {
        if (jsonOutput) {
          console.log(
            JSON.stringify({
              success: false,
              ide: rawIde,
              configPath: null,
              method,
              error: 'npm is not found in PATH',
            })
          );
          process.exitCode = EXIT.GENERAL;
          return;
        }
        console.log();
        console.log(
          `  ${c('yellow', '⚠')} npm is ${c('yellow', 'not found in PATH')}`
        );
        console.log(`  ${dim('npm is required for npx installation method.')}`);
        console.log();
        printNodeDoctorHintCLI();
        process.exitCode = EXIT.GENERAL;
        return;
      }
    }

    if (
      !client ||
      client === 'custom' ||
      !SUPPORTED_INSTALL_CLIENTS.includes(client)
    ) {
      if (jsonOutput) {
        console.log(
          JSON.stringify({
            success: false,
            ide: rawIde,
            configPath: null,
            method,
            error: `Invalid IDE: ${rawIde}. Supported: ${SUPPORTED_INSTALL_CLIENTS_TEXT}`,
          })
        );
        process.exitCode = EXIT.USAGE;
        return;
      }
      console.log();
      console.log(`  ${c('red', '✗')} Invalid IDE: ${rawIde}`);
      console.log(`  ${dim('Supported:')} ${SUPPORTED_INSTALL_CLIENTS_TEXT}`);
      console.log();
      process.exitCode = EXIT.USAGE;
      return;
    }

    if (method !== 'npx') {
      if (jsonOutput) {
        console.log(
          JSON.stringify({
            success: false,
            ide: rawIde,
            configPath: null,
            method,
            error: `Invalid method: ${method}. Supported: npx`,
          })
        );
        process.exitCode = EXIT.USAGE;
        return;
      }
      console.log();
      console.log(`  ${c('red', '✗')} Invalid method: ${method}`);
      console.log(`  ${dim('Supported:')} npx`);
      console.log();
      process.exitCode = EXIT.USAGE;
      return;
    }

    const installMethod = method as InstallMethod;
    const installClient = client as MCPClient;
    const preview = getInstallPreviewForClient(installClient, installMethod);

    if (checkOnly) {
      const cfgPath = preview.configPath;
      const cfgExists = existsSync(cfgPath);
      const existingConfig = cfgExists ? readMCPConfig(cfgPath) : null;
      const hasOctocode = Boolean(existingConfig?.mcpServers?.['octocode-mcp']);
      const parentDir = path.dirname(cfgPath);
      const parentExists = existsSync(parentDir);
      let parentWritable = false;
      if (parentExists) {
        try {
          accessSync(parentDir, constants.W_OK);
          parentWritable = true;
        } catch {
          parentWritable = false;
        }
      } else {
        try {
          accessSync(path.dirname(parentDir), constants.W_OK);
          parentWritable = true;
        } catch {
          parentWritable = false;
        }
      }
      const ready = parentWritable && (preview.action !== 'override' || force);

      if (jsonOutput) {
        console.log(
          JSON.stringify({
            ide: installClient,
            configPath: cfgPath,
            configExists: cfgExists,
            octocodeInstalled: hasOctocode,
            parentDirExists: parentExists,
            parentDirWritable: parentWritable,
            action: preview.action,
            method: installMethod,
            wouldOverwrite: preview.action === 'override',
            ready,
          })
        );
        return;
      }

      console.log();
      console.log(`  ${bold('Install pre-flight check')}`);
      console.log();
      console.log(`  ${dim('IDE:')}    ${getIDEDisplayName(installClient)}`);
      console.log(`  ${dim('Config:')} ${cfgPath}`);
      console.log(
        `  Config exists:       ${cfgExists ? c('green', 'yes') : c('yellow', 'no (will be created)')}`
      );
      console.log(
        `  Parent dir writable: ${parentWritable ? c('green', 'yes') : c('red', 'no — check permissions')}`
      );
      console.log(
        `  Octocode installed:  ${hasOctocode ? c('green', 'yes') : c('yellow', 'no')}`
      );
      console.log(
        `  Action:              ${preview.action === 'override' ? c('yellow', 'overwrite') : c('green', preview.action)}`
      );
      if (!parentWritable) {
        console.log();
        console.log(
          `  ${c('red', '✗')} Cannot write to ${parentDir} — check permissions.`
        );
        process.exitCode = EXIT.GENERAL;
      } else if (preview.action === 'override' && !force) {
        console.log();
        console.log(
          `  ${c('yellow', '⚠')} Already configured. Add --force to overwrite.`
        );
      } else {
        console.log();
        console.log(`  ${c('green', '✓')} Ready to install.`);
      }
      console.log();
      return;
    }

    if (preview.action === 'override' && !force) {
      if (jsonOutput) {
        console.log(
          JSON.stringify({
            success: false,
            ide: installClient,
            configPath: preview.configPath,
            method,
            error: 'Already configured. Use --force to overwrite.',
          })
        );
        process.exitCode = EXIT.USAGE;
        return;
      }
      console.log();
      console.log(`  ${c('yellow', '⚠')} Octocode is already configured.`);
      console.log(
        `  ${dim('Use')} ${c('cyan', '--force')} ${dim('to overwrite.')}`
      );
      console.log();
      process.exitCode = EXIT.USAGE;
      return;
    }

    const spinner = jsonOutput
      ? null
      : new Spinner('Writing configuration...').start();

    const result = installOctocodeForClient({
      client: installClient,
      method: installMethod,
      force,
    });

    if (jsonOutput) {
      console.log(
        JSON.stringify({
          success: result.success,
          ide: installClient,
          configPath: result.success ? preview.configPath : null,
          method,
          backupPath: result.backupPath || null,
          error: result.error || null,
        })
      );
      if (!result.success) process.exitCode = EXIT.GENERAL;
      return;
    }

    if (result.success) {
      spinner?.succeed('Installation complete!');
      console.log();
      console.log(`  ${bold('Installing @octocodeai/mcp')}`);
      console.log(`    ${dim('IDE:')}    ${getIDEDisplayName(installClient)}`);
      console.log(
        `    ${dim('Method:')} ${INSTALL_METHOD_INFO[installMethod].name}`
      );
      console.log();
      console.log(
        `  ${c('green', '✓')} Config saved to: ${preview.configPath}`
      );
      if (result.backupPath) {
        console.log(`  ${dim('Backup:')} ${result.backupPath}`);
      }
      console.log();
      console.log(
        `  ${bold('Next:')} Restart ${getIDEDisplayName(installClient)} to activate.`
      );
      console.log();
    } else {
      spinner?.fail('Installation failed');
      console.log();
      if (result.error) {
        console.log(`  ${c('red', '✗')} ${result.error}`);
      }
      console.log();
      process.exitCode = EXIT.GENERAL;
    }
  },
};
