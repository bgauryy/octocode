// The GitHub token repo scripts send, in the product's order: the token variables of
// octocode-config's config-contract.json by `tokenPriority`, else `gh auth token`.
import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { ROOT } from './workspaces.mjs';

const CONFIG_CONTRACT = path.join(ROOT, 'packages/octocode-config/config-contract.json');

/** The token variable names, highest priority first. */
export function tokenNames(contract = JSON.parse(readFileSync(CONFIG_CONTRACT, 'utf8'))) {
  return Object.entries(contract.environment)
    .filter(([, definition]) => definition.tokenPriority !== undefined)
    .toSorted(([, left], [, right]) => left.tokenPriority - right.tokenPriority)
    .map(([name]) => name);
}

/** The first nonblank token variable, else `gh auth token`; undefined when neither has one. */
export function githubToken(env = process.env) {
  for (const name of tokenNames()) {
    const value = env[name]?.trim();
    if (value) return value;
  }
  const gh = spawnSync('gh', ['auth', 'token'], { encoding: 'utf8' });
  return gh.status === 0 ? gh.stdout.trim() || undefined : undefined;
}
