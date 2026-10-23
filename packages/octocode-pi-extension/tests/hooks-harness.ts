import fs from 'node:fs';
import path from 'node:path';
import { beforeEach, vi } from 'vitest';
import { fingerprint, projectConfigFiles, saveDecision } from '../src/shared/trust.js';
import { tmp } from './helpers.js';

/** A trusted repository whose `file` holds `hooks`. */
export function project(hooks: unknown, file = path.join('.codex', 'hooks.json')): string {
  const dir = tmp('octocode-hooks-');
  fs.mkdirSync(path.join(dir, '.git'));
  fs.mkdirSync(path.dirname(path.join(dir, file)), { recursive: true });
  fs.writeFileSync(path.join(dir, file), JSON.stringify({ hooks }));
  // The user trusted these hooks (`/octocode trust`): Pi's own trust never covers commands it did not show.
  saveDecision(dir, fingerprint(projectConfigFiles(dir)), true);
  return dir;
}

/** Trust decisions go to a per-test store, out of the developer's own. */
export function useHooksHome(): void {
  beforeEach(() => {
    vi.stubEnv('OCTOCODE_HOME', tmp('octocode-hooks-home-'));
  });
}

export const command = (text: string, matcher = '') => [{ matcher, hooks: [{ type: 'command', command: text }] }];
