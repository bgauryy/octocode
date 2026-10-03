// Host adapters invoke Python explicitly: Windows cannot execute a POSIX shebang.
import { existsSync } from 'node:fs';
import { basename, dirname, join } from 'node:path';

export function runtimeCommand(executable, args = []) {
  const sibling = join(dirname(executable), 'communication.py');
  const entry = executable.endsWith('.py') ? executable
    : ['agents-communication', 'agents-communication.ps1'].includes(basename(executable)) && existsSync(sibling) ? sibling : null;
  return entry
    ? { command: process.env.OCTOCODE_PYTHON || (process.platform === 'win32' ? 'python' : 'python3'), args: ['-B', entry, ...args] }
    : { command: executable, args };
}
