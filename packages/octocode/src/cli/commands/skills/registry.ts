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
function parseFrontmatter(content: string): {
  name?: string;
  description?: string;
} {
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
  if (result['description'] !== undefined)
    parsed.description = result['description'];
  return parsed;
}

export interface SkillPathResult {
  skill?: SkillInfo;
  error?: string;
}

/**
 * A skill name is used as a single path segment (`path.join(skillsHome, name)`)
 * everywhere it's resolved to a filesystem location. Restricting it to this
 * charset rejects `/`, `\`, and any leading `.` — which also rules out `..`
 * and hidden-dotfile segments — so a name can never escape its intended
 * directory via path traversal.
 */
const SKILL_NAME_PATTERN = /^[A-Za-z0-9][A-Za-z0-9._-]*$/;

export function isValidSkillName(name: string): boolean {
  return SKILL_NAME_PATTERN.test(name);
}

/** Resolve and validate a standalone local skill folder for `skill --add`. */
export function getSkillFromPath(
  sourcePath: string,
  nameOverride?: string
): SkillPathResult {
  const resolvedInput = path.resolve(sourcePath);
  const skillDir =
    path.basename(resolvedInput).toLowerCase() === 'skill.md'
      ? path.dirname(resolvedInput)
      : resolvedInput;
  const skillMd = path.join(skillDir, 'SKILL.md');

  if (!fs.existsSync(skillMd)) {
    return { error: `SKILL.md not found in: ${skillDir}` };
  }

  let content: string;
  try {
    content = fs.readFileSync(skillMd, 'utf-8');
  } catch (err) {
    return {
      error: `Unable to read ${skillMd}: ${err instanceof Error ? err.message : String(err)}`,
    };
  }

  const frontmatter = parseFrontmatter(content);
  if (!frontmatter.name || !frontmatter.description) {
    return {
      error: `Invalid SKILL.md in ${skillDir}: frontmatter requires name and description.`,
    };
  }

  const folderName = path.basename(skillDir);
  const installName = nameOverride ?? frontmatter.name;
  if (!isValidSkillName(installName)) {
    return { error: `Invalid skill name: "${installName}".` };
  }
  if (!nameOverride && frontmatter.name !== folderName) {
    return {
      error:
        `Skill name mismatch: folder is "${folderName}" but SKILL.md declares ` +
        `"${frontmatter.name}". Pass --name <name> to confirm the install name.`,
    };
  }

  return {
    skill: {
      name: installName,
      folder: installName,
      description: frontmatter.description,
      dir: skillDir,
    },
  };
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
    // After build: out/chunks/<chunk>.js → ../../skills
    path.resolve(thisDir, '..', '..', 'skills'),
    // Non-split build fallback: out/ → ../skills
    path.resolve(thisDir, '..', 'skills'),
    // During dev: src/cli/commands/skills → ../../../../skills
    path.resolve(thisDir, '..', '..', '..', '..', 'skills'),
  ];

  for (const candidate of candidates) {
    if (!fs.existsSync(candidate)) continue;
    const hasSkill = fs
      .readdirSync(candidate, { withFileTypes: true })
      .some(entry =>
        entry.isDirectory()
          ? fs.existsSync(path.join(candidate, entry.name, 'SKILL.md'))
          : false
      );
    if (hasSkill) return candidate;
  }

  // Return first candidate so callers get a meaningful path in error messages
  return candidates[0]!;
}

// ─── Public API ───────────────────────────────────────────────────────────────

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
      s => s.name === nameOrFolder || s.folder === nameOrFolder
    ) ?? null
  );
}

/** Skills removed from the bundle → the bundled skill that now owns their guidance. */
export const RETIRED_SKILLS: Readonly<Record<string, string>> = {
  'octocode-clasify': 'octocode-research',
};

/** Suffix for "not found" errors when the name is a retired skill; empty otherwise. */
export function retiredHint(name: string): string {
  const owner = RETIRED_SKILLS[name];
  return owner
    ? ` "${name}" was retired and merged into "${owner}"; install "${owner}" and run \`octocode skill remove ${name}\`.`
    : '';
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
