/** Bundled skill discovery for CLI help. Runtime assets resolve through skill-paths. */
import { existsSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import { BUNDLED_SKILLS_DIR as RESOLVED_BUNDLED_SKILLS_DIR } from '../skill-paths.js';

export const BUNDLED_SKILLS_DIR = RESOLVED_BUNDLED_SKILLS_DIR;

// ─── Resolved paths ─────────────────────────────────────────────────────────
// Computed once at startup so help text shows real, copy-pasteable paths.

// Awareness is the only package-bundled operating skill. Install other
// workflow skills separately when needed.
export const REQUIRED_BUNDLED_SKILLS = new Set(['octocode-awareness']);

export interface BundledSkill {
  name: string;
  path: string;
  required: boolean;
}

// Discovered at runtime (not hardcoded) so this list can never silently drift
// from whatever build.mjs actually bundled next to this CLI.
export function discoverBundledSkills(skillsDir: string): BundledSkill[] {
  if (!existsSync(skillsDir)) return [];
  return readdirSync(skillsDir, { withFileTypes: true })
    .filter((entry) => entry.isDirectory() && existsSync(join(skillsDir, entry.name, 'SKILL.md')))
    .map((entry) => ({
      name: entry.name,
      path: join(skillsDir, entry.name),
      required: REQUIRED_BUNDLED_SKILLS.has(entry.name),
    }))
    .sort((a, b) => (a.required === b.required ? a.name.localeCompare(b.name) : a.required ? -1 : 1));
}

export const BUNDLED_SKILLS = discoverBundledSkills(BUNDLED_SKILLS_DIR);
