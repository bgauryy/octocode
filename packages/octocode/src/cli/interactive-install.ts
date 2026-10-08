import { spawnSync } from 'node:child_process';
import { select } from '../utils/prompts.js';
import { delegateToNative, nativeCommand } from './native-delegate.js';

function listInstallClients(bin: string): string[] {
  const [command, args] = nativeCommand(bin, ['install', '--list', '--json']);
  const result = spawnSync(command, args, {
    encoding: 'utf8',
    env: process.env,
  });
  if (result.error || result.status !== 0) {
    throw (
      result.error ??
      new Error(result.stderr.trim() || 'Native install discovery failed.')
    );
  }
  const parsed = JSON.parse(result.stdout) as { ides?: unknown };
  if (
    !Array.isArray(parsed.ides) ||
    !parsed.ides.every(value => typeof value === 'string')
  ) {
    throw new Error('Native install discovery returned an invalid response.');
  }
  return parsed.ides;
}

export async function runInteractiveInstall(
  bin: string,
  argv: readonly string[]
): Promise<number> {
  const clients = listInstallClients(bin);
  const ide = await select({
    message: 'Select an IDE or MCP client',
    choices: clients.map(value => ({ name: value, value })),
    pageSize: 12,
  });
  return delegateToNative(bin, [...argv, '--ide', ide]);
}
