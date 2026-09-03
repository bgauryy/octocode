import fs from 'node:fs';
import path from 'node:path';
import { buildAwarenessCommand } from './assets.js';

/** A command to spawn, or an actionable error explaining why it could not be built. */
export type SurfaceSpec = { cmd: string; args: string[] } | { error: string };

/** Octocode launcher surface verbs owned by the core extension. */
export type SurfaceVerb = 'memory' | 'awareness' | 'tools' | 'skills';

/**
 * Build the spawn spec for an Octocode surface verb.
 * `rest` is the user's trailing args (already stripped of the verb token).
 */
export function buildSurfaceSpec(
  verb: SurfaceVerb,
  rest: string[] = [],
  _env: NodeJS.ProcessEnv = process.env,
): SurfaceSpec {
  switch (verb) {
    case 'memory':
    case 'awareness': {
      const prefix = verb === 'memory' ? ['memory'] : [];
      return buildAwarenessCommand([...prefix, ...rest]);
    }
    case 'tools':
      return { cmd: 'npx', args: ['octocode', 'tools', ...rest] };
    case 'skills':
      return { cmd: 'npx', args: ['octocode', 'skill', ...rest] };
    default: {
      const _exhaustive: never = verb;
      return { error: `Unknown surface verb: ${String(_exhaustive)}` };
    }
  }
}

/** A named preset: model + permission + tool scoping, applied as Pi flags at launch. */
export interface Profile {
  model?: string;
  tools?: string;
  excludeTools?: string;
  /** 'always' -> non-interactive trust (-a); 'never' -> -na; 'ask' -> no flag. */
  approve?: 'always' | 'never' | 'ask';
}

/** Load a named profile from <home>/profiles.json, or null when absent/invalid. */
export function loadProfile(name: string, home: string): Profile | null {
  const file = path.join(home, 'profiles.json');
  if (!fs.existsSync(file)) return null;
  try {
    const all = JSON.parse(fs.readFileSync(file, 'utf8')) as Record<string, Profile>;
    return all[name] ?? null;
  } catch {
    return null;
  }
}

/** Translate a profile into Pi CLI flags (order-stable). */
export function profileToPiArgs(profile: Profile): string[] {
  const args: string[] = [];
  if (profile.model) args.push('--model', profile.model);
  if (profile.tools) args.push('--tools', profile.tools);
  if (profile.excludeTools) args.push('--exclude-tools', profile.excludeTools);
  if (profile.approve === 'always') args.push('-a');
  else if (profile.approve === 'never') args.push('-na');
  return args;
}
