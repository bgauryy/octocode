import { spawnSync } from 'node:child_process';
import { select } from '../utils/prompts.js';
import { delegateToNative } from './native-delegate.js';

function nativeCommand(
  bin: string,
  argv: readonly string[]
): readonly [string, string[]] {
  return bin.endsWith('.cjs') || bin.endsWith('.js')
    ? [process.execPath, [bin, ...argv]]
    : [bin, [...argv]];
}

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
  const parsed = JSON.parse(result.stdout) as { supported?: unknown };
  if (
    !Array.isArray(parsed.supported) ||
    !parsed.supported.every(value => typeof value === 'string')
  ) {
    throw new Error('Native install discovery returned an invalid response.');
  }
  return parsed.supported;
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
