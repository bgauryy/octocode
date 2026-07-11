import type { MCPClient } from '../../../types/index.js';
import { c, bold, dim } from '../../../utils/colors.js';
import { getMCPConfigPath } from '../../../utils/mcp-paths.js';
import { existsSync, copyFileSync, mkdirSync } from 'node:fs';
import { EXIT } from '../../exit-codes.js';
import path from 'node:path';
import { getIDEDisplayName } from '../shared.js';

export interface RunRollbackParams {
  client: MCPClient;
  rawIde: string;
  rawBackupPath: string | boolean | undefined;
  jsonOutput: boolean;
}

/**
 * Handles `install --rollback`: restores a previously backed-up MCP config
 * file over the current one. Always terminates the command (no further
 * handling is needed by the caller after this returns).
 */
export function runRollback({
  client,
  rawIde,
  rawBackupPath,
  jsonOutput,
}: RunRollbackParams): void {
  const cfgPath = getMCPConfigPath(client);
  const backupPath =
    typeof rawBackupPath === 'string' && rawBackupPath.length > 0
      ? rawBackupPath
      : `${cfgPath}.bak`;

  if (!existsSync(backupPath)) {
    if (jsonOutput) {
      console.log(
        JSON.stringify({
          success: false,
          ide: rawIde,
          configPath: cfgPath,
          backupPath,
          error: `Backup not found: ${backupPath}`,
        })
      );
    } else {
      console.log();
      console.log(`  ${c('red', '✗')} Backup not found: ${backupPath}`);
      console.log(`  ${dim('Provide the path via --backup-path <path>')}`);
      console.log();
    }
    process.exitCode = EXIT.NOT_FOUND;
    return;
  }

  try {
    mkdirSync(path.dirname(cfgPath), { recursive: true, mode: 0o700 });
    copyFileSync(backupPath, cfgPath);
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    if (jsonOutput) {
      console.log(
        JSON.stringify({
          success: false,
          ide: rawIde,
          configPath: cfgPath,
          backupPath,
          error: msg,
        })
      );
    } else {
      console.log();
      console.log(`  ${c('red', '✗')} Rollback failed: ${msg}`);
      console.log();
    }
    process.exitCode = EXIT.GENERAL;
    return;
  }

  if (jsonOutput) {
    console.log(
      JSON.stringify({
        success: true,
        ide: rawIde,
        configPath: cfgPath,
        backupPath,
      })
    );
  } else {
    console.log();
    console.log(
      `  ${c('green', '✓')} Rolled back ${c('cyan', rawIde)} config from backup`
    );
    console.log(`  ${dim('Config:')}  ${cfgPath}`);
    console.log(`  ${dim('Backup:')} ${backupPath}`);
    console.log();
    console.log(
      `  ${bold('Next:')} Restart ${getIDEDisplayName(client)} to apply.`
    );
    console.log();
  }
}
