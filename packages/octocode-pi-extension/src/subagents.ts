/**
 * Subagent registry — typed configuration for every specialised Pi subagent
 * this extension ships.
 *
 * Each subagent has:
 *   - A typed name (union literal)
 *   - Tool allowlist (no nested spawning; write tools stay out unless a role explicitly needs them)
 *   - Resource mode (always 'octocode' so the extension's own tools are available)
 *   - SYSTEM_PROMPT.md path loaded at runtime from dist/subagents/<name>/
 *   - All bundled Octocode skills, plus any subagent-local skill dirs
 *
 * The spawnSubagent tool reads this registry, loads the system prompt,
 * and calls spawnRpcAgent (same internal fn as spawnAgent, same agents Map →
 * AgentMessage works on anything spawned here).
 */

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import type { ResourceMode } from './tools/agent-tools.js';

// ─── Types ────────────────────────────────────────────────────────────────────

export interface SubagentConfig {
  /** Unique id — used as the spawnSubagent `agent` param value. */
  name: SubagentName;
  /** Human label shown in AgentMessage list output. */
  label: string;
  /** One-line description of what this subagent does. */
  description: string;
  /**
   * Tool allowlist for the subprocess. spawnAgent/AgentMessage are always
   * excluded by Pi regardless.
   */
  tools: string[];
  /**
   * Resource mode for the subprocess.
   * 'octocode' loads this extension so the subagent has chromeDebug etc.
   * 'lean' = no extensions, no skills — only built-in tools.
   */
  resourceMode: ResourceMode;
  /** Thinking level for the subprocess. */
  thinking?: string;
  /** Default model override. */
  model?: string;
  /**
   * Absolute path to SYSTEM_PROMPT.md for this subagent.
   * Loaded at runtime from dist/subagents/<name>/SYSTEM_PROMPT.md.
   */
  systemPromptPath: string;
  /** Skill dirs passed via --skill (loaded even with --no-skills). */
  skills?: string[];
}

/** Union of all registered subagent names (extend when adding new subagents). */
export type SubagentName =
  'browser-agent' | 'researcher' | 'planner' | 'architect';

// ─── Runtime path resolution ──────────────────────────────────────────────────

function resolveSubagentsDir(): string {
  const moduleDir = path.dirname(fileURLToPath(import.meta.url));
  const distDir = path.join(moduleDir, 'subagents');
  if (fs.existsSync(distDir)) return distDir;
  return path.resolve(moduleDir, '..', 'subagents');
}

/** dist/subagents/ in published builds; packageRoot/subagents/ in source tests. */
const SUBAGENTS_DIR = resolveSubagentsDir();

function resolveSkillsDir(): string {
  const siblingSkillsDir = path.join(path.dirname(SUBAGENTS_DIR), 'skills');
  if (fs.existsSync(siblingSkillsDir)) return siblingSkillsDir;
  return path.resolve(path.dirname(SUBAGENTS_DIR), '..', '..', 'skills');
}

const SKILLS_DIR = resolveSkillsDir();

export const OCTOCODE_SKILL_NAMES = [
  'octocode-awareness',
  'octocode-brainstorming',
  'octocode-prompt-optimizer',
  'octocode-research',
  'octocode-rfc-generator',
  'octocode-roast',
  'octocode-skills',
] as const;

function subagentSkillPath(name: SubagentName, skillName: string): string {
  return path.join(SUBAGENTS_DIR, name, 'skills', skillName);
}

function bundledSkillPath(
  skillName: (typeof OCTOCODE_SKILL_NAMES)[number]
): string {
  return path.join(SKILLS_DIR, skillName);
}

function allOctocodeSkillPaths(...extraSkillPaths: string[]): string[] {
  return [
    ...OCTOCODE_SKILL_NAMES.map(skillName => bundledSkillPath(skillName)),
    ...extraSkillPaths,
  ];
}

function subagentPromptPath(name: SubagentName): string {
  return path.join(SUBAGENTS_DIR, name, 'SYSTEM_PROMPT.md');
}

export function loadSystemPrompt(config: SubagentConfig): string {
  const p = config.systemPromptPath;
  if (!fs.existsSync(p)) {
    throw new Error(
      `subagent system prompt not found: ${p}\n` +
        `Run: yarn workspace @octocodeai/pi-extension build`
    );
  }
  return fs.readFileSync(p, 'utf8');
}

// ─── Registry ─────────────────────────────────────────────────────────────────

export const SUBAGENT_REGISTRY = {
  'browser-agent': {
    name: 'browser-agent' as SubagentName,
    label: 'Browser Agent',
    description:
      'Specialised browser debugging subagent. Has chromeDebug + web + local search tools. ' +
      'Use for multi-turn Chrome DevTools Protocol work: security audits, network analysis, ' +
      'DOM inspection, coverage, workers, service workers, emulation, and automation.',
    tools: [
      'chromeDebug', // CDP execution — primary tool
      'web', // CDP docs + web research
      'localGetFileContent', // read source files, screenshots
      'localSearchCode', // correlate browser errors to local source
      'localViewStructure', // navigate file trees
    ],
    resourceMode: 'octocode' as ResourceMode,
    thinking: 'low',
    systemPromptPath: subagentPromptPath('browser-agent'),
    skills: allOctocodeSkillPaths(
      subagentSkillPath('browser-agent', 'browser-agent')
    ),
  },
  researcher: {
    name: 'researcher' as SubagentName,
    label: 'Researcher',
    description:
      'Fast Octocode research specialist. Has web, GitHub, npm, local, binary, and LSP tools. ' +
      'Use for evidence gathering, prior art, package/repo lookup, and concise claim ledgers.',
    tools: [
      'web',
      'ghSearchCode',
      'ghGetFileContent',
      'ghViewRepoStructure',
      'ghSearchRepos',
      'ghHistoryResearch',
      'ghCloneRepo',
      'npmSearch',
      'localSearchCode',
      'localViewStructure',
      'localFindFiles',
      'localGetFileContent',
      'localBinaryInspect',
      'lspGetSemantics',
    ],
    resourceMode: 'octocode' as ResourceMode,
    thinking: 'low',
    systemPromptPath: subagentPromptPath('researcher'),
    skills: allOctocodeSkillPaths(),
  },
  planner: {
    name: 'planner' as SubagentName,
    label: 'Planner',
    description:
      'Implementation planning specialist. Has all Octocode research surfaces and all bundled skills. ' +
      'Use for dependency-ordered plans, risks, verification strategy, and RFC handoff packets.',
    tools: [
      'web',
      'ghSearchCode',
      'ghGetFileContent',
      'ghViewRepoStructure',
      'ghSearchRepos',
      'ghHistoryResearch',
      'ghCloneRepo',
      'npmSearch',
      'localSearchCode',
      'localViewStructure',
      'localFindFiles',
      'localGetFileContent',
      'localBinaryInspect',
      'lspGetSemantics',
    ],
    resourceMode: 'octocode' as ResourceMode,
    thinking: 'low',
    systemPromptPath: subagentPromptPath('planner'),
    skills: allOctocodeSkillPaths(),
  },
  architect: {
    name: 'architect' as SubagentName,
    label: 'Architect',
    description:
      'Root-cause and local-code architecture specialist. Has all Octocode skills, local/LSP/binary tools, ' +
      'GitHub history, web, and bash for targeted debug/test loops.',
    tools: [
      'bash',
      'web',
      'ghSearchCode',
      'ghGetFileContent',
      'ghViewRepoStructure',
      'ghSearchRepos',
      'ghHistoryResearch',
      'ghCloneRepo',
      'npmSearch',
      'localSearchCode',
      'localViewStructure',
      'localFindFiles',
      'localGetFileContent',
      'localBinaryInspect',
      'lspGetSemantics',
    ],
    resourceMode: 'octocode' as ResourceMode,
    thinking: 'medium',
    systemPromptPath: subagentPromptPath('architect'),
    skills: allOctocodeSkillPaths(),
  },
} satisfies Record<SubagentName, SubagentConfig>;

export const SUBAGENT_NAMES = Object.keys(SUBAGENT_REGISTRY) as SubagentName[];
