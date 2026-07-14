/**
 * Skill registry — reads bundled skills and parses SKILL.md frontmatter.
 *
 * Skills are bundled into skills/ relative to the package root at build time.
 * Each skill folder must contain a SKILL.md with YAML frontmatter:
 *
 *   ---
 *   name: octocode-research
 *   description: "..."
 *   ---
 */

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

export interface SkillInfo {
  /** Name from SKILL.md frontmatter (e.g. "octocode-research") */
  name: string;
  /** Folder name in the skills directory (same as name for official skills) */
  folder: string;
  /** Short description from SKILL.md frontmatter */
  description: string;
  /** Absolute path to the skill directory */
  dir: string;
}

// ─── SKILL.md frontmatter parser ─────────────────────────────────────────────

/**
 * Minimal YAML frontmatter parser.
 * Handles: name: value, description: "quoted value"
 * Does NOT handle multi-line values — not needed for SKILL.md.
 */
function parseFrontmatter(content: string): { name?: string; description?: string } {
  const match = /^---\s*\n([\s\S]*?)\n---/.exec(content);
  if (!match || !match[1]) return {};

  const result: Record<string, string> = {};

  for (const line of match[1].split('\n')) {
    const colon = line.indexOf(':');
    if (colon < 0) continue;
    const key = line.slice(0, colon).trim();
    const rawValue = line.slice(colon + 1).trim();
    // Strip surrounding quotes
    const value = rawValue.replace(/^["']|["']$/g, '');
    if (key) result[key] = value;
  }

  const parsed: { name?: string; description?: string } = {};
  if (result['name'] !== undefined) parsed.name = result['name'];
  if (result['description'] !== undefined) parsed.description = result['description'];
  return parsed;
}

// ─── Skills directory resolution ─────────────────────────────────────────────

/**
 * Locate the bundled skills directory.
 *
 * When built: out/cli.js → skills/ is at package-root/skills/
 * In development (ts-node / vitest): src/*.ts → skills/ at package-root/skills/
 */
function findBundledSkillsDir(): string {
  const thisFile = fileURLToPath(import.meta.url);
  const thisDir = path.dirname(thisFile);

  const candidates = [
    // After build: out/ → ../skills
    path.resolve(thisDir, '..', 'skills'),
    // During dev: src/ → ../skills (same level)
    path.resolve(thisDir, 'skills'),
  ];

  for (const candidate of candidates) {
    if (fs.existsSync(candidate)) return candidate;
  }

  // Return first candidate so callers get a meaningful path in error messages
  return candidates[0]!;
}

// ─── Public API ───────────────────────────────────────────────────────────────

/** Absolute path to the bundled skills directory. */
export function getBundledSkillsDir(): string {
  return findBundledSkillsDir();
}

let _skillsCache: SkillInfo[] | null = null;

/**
 * List all bundled skills.
 * Only directories with a valid SKILL.md (name + description) are included.
 * Result is memoized for the process lifetime — skills are static during a CLI run.
 */
export function listSkills(): SkillInfo[] {
  if (_skillsCache) return _skillsCache;
  const skillsDir = findBundledSkillsDir();

  if (!fs.existsSync(skillsDir)) return [];

  const entries = fs.readdirSync(skillsDir, { withFileTypes: true });
  const skills: SkillInfo[] = [];

  for (const entry of entries) {
    if (!entry.isDirectory() && !entry.isSymbolicLink()) continue;

    const skillDir = path.join(skillsDir, entry.name);
    const skillMd = path.join(skillDir, 'SKILL.md');

    if (!fs.existsSync(skillMd)) continue;

    let content: string;
    try {
      content = fs.readFileSync(skillMd, 'utf-8');
    } catch {
      continue;
    }

    const { name, description } = parseFrontmatter(content);
    if (!name || !description) continue;

    skills.push({
      name,
      folder: entry.name,
      description,
      dir: skillDir,
    });
  }

  _skillsCache = skills.sort((a, b) => a.name.localeCompare(b.name));
  return _skillsCache;
}

/**
 * Look up a skill by name or folder name.
 */
export function getSkill(nameOrFolder: string): SkillInfo | null {
  return (
    listSkills().find(
      (s) => s.name === nameOrFolder || s.folder === nameOrFolder
    ) ?? null
  );
}

/**
 * Read the full SKILL.md content for a skill (for the `info` command).
 */
export function getSkillContent(skill: SkillInfo): string | null {
  const skillMd = path.join(skill.dir, 'SKILL.md');
  try {
    return fs.readFileSync(skillMd, 'utf-8');
  } catch {
    return null;
  }
}
