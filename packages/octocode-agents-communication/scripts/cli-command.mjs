// Select script interpreters explicitly instead of relying on executable shebangs.
import { existsSync, realpathSync } from 'node:fs';
import { basename, dirname, join } from 'node:path';

export function runtimeCommand(executable, args = []) {
  const sibling = join(dirname(existsSync(executable) ? realpathSync(executable) : executable), 'communication.py');
  const entry = executable.endsWith('.py') ? executable
    : ['agents-communication', 'agents-communication.ps1'].includes(basename(executable)) && existsSync(sibling) ? sibling : null;
  if (entry) return { command: process.env.OCTOCODE_PYTHON || (process.platform === 'win32' ? 'python' : 'python3'), args: ['-B', entry, ...args] };
  if (/\.[cm]js$/.test(executable)) return { command: process.execPath, args: [executable, ...args] };
  return { command: executable, args };
}
