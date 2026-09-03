/**
 * dynamic-skills — the deterministic core of the unified `skill` call lifecycle.
 *
 * A "dynamic skill" is an approved, reusable **workflow** the agent follows: a directory
 * with a `SKILL.md` (Agent Skills frontmatter + markdown steps) and optional `scripts/`.
 * Unlike a dynamic tool (deterministic code the harness runs), a skill is guidance the
 * model reads and follows via `read` / `/skill:<name>`. Any executable `scripts/` a skill
 * ships should be run through the callTool sandbox — skills orchestrate, tools execute.
 *
 * Skills are written to the user's Pi skill directory so Pi discovers them:
 *   - `~/.pi/agent/skills/<name>/` (default) — surfaced to spawned subagents immediately
 *     (their skill dirs are re-scanned per spawn); the main process needs a restart or a
 *     direct `read` of the returned SKILL.md path to surface it in its own prompt.
 *
 * This module owns everything deterministic and unit-testable: registry read/write,
 * O(1) resolve, frontmatter+structure validation (the skill verification gate, weaker
 * than a tool's test gate), CRUD delete, and a junk sweep. Authoring (writing SKILL.md
 * content) needs an LLM and lives in the private call orchestrator via a skill-smith subagent.
 */

import fs from 'node:fs';
import path from 'node:path';
import { parseAgentSkill } from '@octocodeai/octocode-shared/agent-skills';
import { getPiUserSkillsDir } from '../utils.js';
import { KEYWORD_MATCH_THRESHOLD, tokenize, withRegistryLock, writeJsonAtomic, readJsonSafe } from './registry-store.js';
import { ensurePrivateDirectory, hardenPrivateFile, PRIVATE_FILE_MODE } from '@octocodeai/octocode-awareness/mcp-state';

// ─── types ────────────────────────────────────────────────────────────────────

export interface SkillManifestEntry {
  name: string;
  description: string;
  /** Absolute path to the skill directory (contains SKILL.md). */
  dir: string;
  /** Absolute path to SKILL.md (what the agent `read`s to follow the workflow). */
  skillMd: string;
  /** Why this workflow deserves a persisted, reusable skill. */
  reason: string;
  version: number;
  createdAt: string;
  updatedAt: string;
  stats: { uses: number; lastUsedAt: string | null };
}

interface SkillIndex {
  version: 1;
  skills: Record<string, SkillManifestEntry>;
}

export type SkillResolveResult =
  | { hit: 'exact'; entry: SkillManifestEntry }
  | { hit: 'keyword'; entry: SkillManifestEntry; score: number }
  | { hit: 'miss' };

export interface SkillInput {
  name: string;
  description: string;
  reason: string;
  /** Full SKILL.md content (including frontmatter). */
  skillMd: string;
  /** Optional helper files, written relative to the skill dir (e.g. `scripts/run.mjs`). */
  files?: Array<{ relPath: string; content: string }>;
}

export type SkillRegisterResult =
  | { ok: true; entry: SkillManifestEntry }
  | { ok: false; reason: 'invalid-name' | 'no-reason' | 'invalid-frontmatter' | 'invalid-structure'; detail?: string };

// Agent Skills spec: 1-64 chars, lowercase a-z/0-9/hyphen, no leading/trailing/double hyphen.
const SKILL_NAME_RE = /^[a-z0-9]+(?:-[a-z0-9]+)*$/;
const MAX_NAME = 64;
const MAX_DESCRIPTION = 1024;

// ─── paths ──────────────────────────────────────────────────────────────────

/**
 * Skills registry root. Defaults to `~/.pi/agent/skills` so Pi discovers created skills.
 * Overridable via env for tests / non-default homes.
 */
export function getSkillsDir(env: NodeJS.ProcessEnv = process.env): string {
  const override = env.OCTOCODE_DYNAMIC_SKILLS_DIR;
  if (override) return override;
  return getPiUserSkillsDir(env.HOME || env.USERPROFILE || process.cwd());
}

/** The registry index lives beside the skill dirs but is ignored by Pi's SKILL.md scan. */
function indexPath(dir: string): string {
  return path.join(dir, '.octocode-skills-index.json');
}

function skillDir(dir: string, name: string): string {
  return path.join(dir, name);
}

// ─── registry io (atomic) ─────────────────────────────────────────────────────

function ensureRegistry(dir: string): void {
  ensurePrivateDirectory(dir);
  if (!fs.existsSync(indexPath(dir))) writeIndex(dir, { version: 1, skills: {} });
}

export function readIndex(dir = getSkillsDir()): SkillIndex {
  ensureRegistry(dir);
  return readJsonSafe<SkillIndex>(
    indexPath(dir),
    { version: 1, skills: {} },
    (raw) => Boolean((raw as SkillIndex).skills),
  );
}

function writeIndex(dir: string, idx: SkillIndex): void {
  writeJsonAtomic(indexPath(dir), idx);
}

/** Cross-process mutex around a read-modify-write of the shared skills registry. */
function withIndexLock<T>(dir: string, fn: () => T): T {
  return withRegistryLock(dir, '.skills-index.lock', 'dynamic-skills', fn);
}

// ─── validation (the skill verification gate) ─────────────────────────────────

export interface Frontmatter {
  name?: string;
  description?: string;
  [k: string]: unknown;
}

/**
 * Parse the leading `--- ... ---` YAML frontmatter via pi's canonical yaml-based parser.
 * Returns null when no frontmatter block is present (preserves null-on-absent semantics).
 * Replaces the former hand-rolled regex parser; uses the same implementation Pi itself
 * uses when loading skills, ensuring byte-identical field extraction.
 */
export function parseFrontmatter(skillMd: string): Frontmatter | null {
  const parsed = parseAgentSkill(skillMd);
  if (!parsed.ok) return null;
  return {
    name: parsed.skill.name,
    description: parsed.skill.description,
    ...(parsed.skill.license ? { license: parsed.skill.license } : {}),
    ...(parsed.skill.compatibility ? { compatibility: parsed.skill.compatibility } : {}),
    ...(parsed.skill.metadata ? { metadata: parsed.skill.metadata } : {}),
    ...(parsed.skill.allowedTools ? { 'allowed-tools': parsed.skill.allowedTools } : {}),
  };
}

/**
 * Validate a candidate skill. Skills have no runnable "test passes" gate like tools, so
 * this enforces the Agent Skills contract deterministically: valid frontmatter (name +
 * description within limits) and a minimal structure (a heading and some body). Quality
 * (rubric) and user approval are layered on top in the tool.
 */
export type SkillValidation =
  | { ok: true }
  | { ok: false; reason: 'invalid-name' | 'no-reason' | 'invalid-frontmatter' | 'invalid-structure'; detail?: string };

export function validateSkill(input: SkillInput): SkillValidation {
  if (!input.name || input.name.length > MAX_NAME || !SKILL_NAME_RE.test(input.name)) {
    return { ok: false, reason: 'invalid-name', detail: input.name };
  }
  if (!input.reason || !input.reason.trim()) {
    return { ok: false, reason: 'no-reason' };
  }
  const parsedSkill = parseAgentSkill(input.skillMd, input.name);
  const fm = parsedSkill.ok ? parsedSkill.skill : null;
  if (!fm || !fm.name || !fm.description) {
    return { ok: false, reason: 'invalid-frontmatter', detail: 'missing name/description frontmatter' };
  }
  if (String(fm.description).length > MAX_DESCRIPTION) {
    return { ok: false, reason: 'invalid-frontmatter', detail: 'description exceeds 1024 chars' };
  }
  // Structure: a body after the frontmatter with at least one heading and real content.
  const body = parsedSkill.ok ? parsedSkill.skill.body.trim() : '';
  if (!/^#\s+\S/m.test(body) || body.length < 40) {
    return { ok: false, reason: 'invalid-structure', detail: 'SKILL.md needs a heading and substantive steps' };
  }
  return { ok: true };
}

// ─── resolve (O(1) exact + keyword fallback) ──────────────────────────────────

export function resolveSkill(
  skillType: string,
  intent = '',
  dir = getSkillsDir(),
): SkillResolveResult {
  const idx = readIndex(dir);
  const exact = idx.skills[skillType];
  if (exact) return { hit: 'exact', entry: exact };

  const tokens = tokenize(`${skillType} ${intent}`);
  let best: { entry: SkillManifestEntry; score: number } | null = null;
  for (const entry of Object.values(idx.skills)) {
    const words = tokenize(`${entry.name} ${entry.description}`);
    let score = 0;
    for (const t of tokens) if (words.has(t)) score++;
    if (!best || score > best.score) best = { entry, score };
  }
  if (best && best.score >= KEYWORD_MATCH_THRESHOLD) return { hit: 'keyword', entry: best.entry, score: best.score };
  return { hit: 'miss' };
}



// ─── registration (validation-gated) ──────────────────────────────────────────

export function registerSkill(input: SkillInput, dir = getSkillsDir()): SkillRegisterResult {
  const valid = validateSkill(input);
  if (!valid.ok) return valid as SkillRegisterResult;

  ensureRegistry(dir);
  const sdir = skillDir(dir, input.name);
  const existing = readIndex(dir).skills[input.name];
  ensurePrivateDirectory(sdir);
  const skillMdPath = path.join(sdir, 'SKILL.md');
  fs.writeFileSync(skillMdPath, input.skillMd, { mode: PRIVATE_FILE_MODE });
  hardenPrivateFile(skillMdPath);
  for (const f of input.files ?? []) {
    const target = path.join(sdir, f.relPath);
    if (!path.resolve(target).startsWith(path.resolve(sdir) + path.sep)) continue; // no escapes
    ensurePrivateDirectory(path.dirname(target));
    fs.writeFileSync(target, f.content, { mode: PRIVATE_FILE_MODE });
    hardenPrivateFile(target);
  }

  const now = new Date().toISOString();
  const entry: SkillManifestEntry = {
    name: input.name,
    description: input.description,
    dir: sdir,
    skillMd: skillMdPath,
    reason: input.reason.trim(),
    version: existing ? existing.version + 1 : 1,
    createdAt: existing?.createdAt ?? now,
    updatedAt: now,
    stats: existing?.stats ?? { uses: 0, lastUsedAt: null },
  };
  withIndexLock(dir, () => {
    const idx = readIndex(dir);
    idx.skills[input.name] = entry;
    writeIndex(dir, idx);
  });
  return { ok: true, entry };
}

// ─── CRUD + maintenance ────────────────────────────────────────────────────────

export function listSkills(dir = getSkillsDir()): SkillManifestEntry[] {
  return Object.values(readIndex(dir).skills);
}

export function deleteSkill(name: string, dir = getSkillsDir()): boolean {
  return withIndexLock(dir, () => {
    const idx = readIndex(dir);
    if (!idx.skills[name]) return false;
    delete idx.skills[name];
    writeIndex(dir, idx);
    fs.rmSync(skillDir(dir, name), { recursive: true, force: true });
    return true;
  });
}

export function recordSkillUse(name: string, dir = getSkillsDir()): void {
  withIndexLock(dir, () => {
    const idx = readIndex(dir);
    const entry = idx.skills[name];
    if (!entry) return;
    entry.stats.uses += 1;
    entry.stats.lastUsedAt = new Date().toISOString();
    writeIndex(dir, idx);
  });
}

/** Prune junk: an index entry whose SKILL.md is missing/unreadable, or invalid frontmatter. */
export function sweepJunkSkills(dir = getSkillsDir()): string[] {
  const pruned: string[] = [];
  for (const entry of listSkills(dir)) {
    let broken = !fs.existsSync(entry.skillMd);
    if (!broken) {
      try {
        const fm = parseFrontmatter(fs.readFileSync(entry.skillMd, 'utf8'));
        broken = !fm || !fm.name || !fm.description;
      } catch {
        broken = true;
      }
    }
    if (broken && deleteSkill(entry.name, dir)) pruned.push(entry.name);
  }
  return pruned;
}
