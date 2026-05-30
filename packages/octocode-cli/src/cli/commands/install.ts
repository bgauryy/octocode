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
import { runInteractiveMode } from '../../interactive.js';
import {
  formatSupportedMCPClients,
  getIDEDisplayName,
  normalizeMCPClient,
  printNodeDoctorHintCLI,
} from './shared.js';
import { DETECTABLE_MCP_CLIENTS } from '../../utils/mcp-paths.js';

const SUPPORTED_INSTALL_CLIENTS = DETECTABLE_MCP_CLIENTS;
const SUPPORTED_INSTALL_CLIENTS_TEXT = formatSupportedMCPClients({
  includeInstallAlias: true,
});

export const installCommand: CLICommand = {
  name: 'install',
  aliases: ['i', 'setup'],
  description: 'Install octocode-mcp for an IDE',
  usage: 'octocode-cli install --ide <ide> [--method <npx|direct>] [--force]',
  options: [
    {
      name: 'ide',
      description: `IDE to configure: ${SUPPORTED_INSTALL_CLIENTS_TEXT}`,
      hasValue: true,
    },
    {
      name: 'method',
      short: 'm',
      description: 'Installation method (npx or direct)',
      hasValue: true,
      default: 'npx',
    },
    {
      name: 'force',
      short: 'f',
      description: 'Overwrite existing configuration',
    },
  ],
  handler: async (args: ParsedArgs) => {
    const rawIde = args.options['ide'];
    const methodOpt = args.options['method'] ?? args.options['m'];
    const method = (typeof methodOpt === 'string' ? methodOpt : 'npx') as
      | InstallMethod
      | string;
    const force = Boolean(args.options['force'] || args.options['f']);

    if (typeof rawIde !== 'string' || rawIde.trim().length === 0) {
      await runInteractiveMode();
      return;
    }

    const client = normalizeMCPClient(rawIde);

    if (method === 'npx') {
      const nodeCheck = checkNodeInPath();
      const npmCheck = checkNpmInPath();

      if (!nodeCheck.installed) {
        console.log();
        console.log(
          `  ${c('red', '✗')} Node.js is ${c('red', 'not found in PATH')}`
        );
        console.log(
          `  ${dim('Node.js is required for npx installation method.')}`
        );
        console.log();
        printNodeDoctorHintCLI();
        process.exitCode = 1;
        return;
      }

      if (!npmCheck.installed) {
        console.log();
        console.log(
          `  ${c('yellow', '⚠')} npm is ${c('yellow', 'not found in PATH')}`
        );
        console.log(`  ${dim('npm is required for npx installation method.')}`);
        console.log();
        printNodeDoctorHintCLI();
        process.exitCode = 1;
        return;
      }
    }

    if (
      !client ||
      client === 'custom' ||
      !SUPPORTED_INSTALL_CLIENTS.includes(client)
    ) {
      console.log();
      console.log(`  ${c('red', '✗')} Invalid IDE: ${rawIde}`);
      console.log(`  ${dim('Supported:')} ${SUPPORTED_INSTALL_CLIENTS_TEXT}`);
      console.log();
      process.exitCode = 1;
      return;
    }

    if (!['npx', 'direct'].includes(method)) {
      console.log();
      console.log(`  ${c('red', '✗')} Invalid method: ${method}`);
      console.log(`  ${dim('Supported:')} npx, direct`);
      console.log();
      process.exitCode = 1;
      return;
    }

    const installMethod = method as InstallMethod;
    const installClient = client as MCPClient;
    const preview = getInstallPreviewForClient(installClient, installMethod);

    if (preview.action === 'override' && !force) {
      console.log();
      console.log(`  ${c('yellow', '⚠')} Octocode is already configured.`);
      console.log(
        `  ${dim('Use')} ${c('cyan', '--force')} ${dim('to overwrite.')}`
      );
      console.log();
      process.exitCode = 1;
      return;
    }

    console.log();
    console.log(`  ${bold('Installing octocode-mcp')}`);
    console.log(`    ${dim('IDE:')}    ${getIDEDisplayName(installClient)}`);
    console.log(
      `    ${dim('Method:')} ${INSTALL_METHOD_INFO[installMethod].name}`
    );
    console.log(`    ${dim('Action:')} ${preview.action.toUpperCase()}`);
    console.log();

    const spinner = new Spinner('Writing configuration...').start();

    const result = installOctocodeForClient({
      client: installClient,
      method: installMethod,
      force,
    });

    if (result.success) {
      spinner.succeed('Installation complete!');
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
      spinner.fail('Installation failed');
      console.log();
      if (result.error) {
        console.log(`  ${c('red', '✗')} ${result.error}`);
      }
      console.log();
      process.exitCode = 1;
    }
  },
};
