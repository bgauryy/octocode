import { readdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

export const root = dirname(dirname(fileURLToPath(import.meta.url)));

/** Library entry modules by dist bundle name: every src/entries/<name>.ts is dist/<name>.js and every src/hosts/<name>.ts is dist/hosts/<name>.js. */
export function libraryEntries(): Record<string, string> {
  const sources = (directory: string, prefix: string) => readdirSync(join(root, 'src', directory)).filter(name => name.endsWith('.ts'))
    .map((name): [string, string] => [`${prefix}${name.slice(0, -3)}`, join(root, 'src', directory, name)]);
  return Object.fromEntries([...sources('entries', ''), ...sources('hosts', 'hosts/')]);
}
