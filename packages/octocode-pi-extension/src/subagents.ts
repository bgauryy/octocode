/**
 * Subagent registry — typed configuration for every specialised Pi subagent
 * this extension ships.
 *
 * Each subagent has:
 *   - A typed name (union literal)
 *   - Tool allowlist (no memory/awareness, no bash/edit/write, no nested spawning)
 *   - Resource mode (always 'octocode' so the extension's own tools are available)
 *   - SYSTEM_PROMPT.md path loaded at runtime from dist/subagents/<name>/
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
   * Tool allowlist for the subprocess.
   * spawnAgent/AgentMessage are always excluded by Pi regardless.
   * Omit memory_* tools intentionally — subagents are stateless.
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
export type SubagentName = 'browser-agent';

// ─── Runtime path resolution ──────────────────────────────────────────────────

function resolveSubagentsDir(): string {
  const moduleDir = path.dirname(fileURLToPath(import.meta.url));
  const distDir = path.join(moduleDir, 'subagents');
  if (fs.existsSync(distDir)) return distDir;
  return path.resolve(moduleDir, '..', 'subagents');
}

/** dist/subagents/ in published builds; packageRoot/subagents/ in source tests. */
const SUBAGENTS_DIR = resolveSubagentsDir();

function subagentSkillPath(name: SubagentName, skillName: string): string {
  return path.join(SUBAGENTS_DIR, name, 'skills', skillName);
}

function subagentPromptPath(name: SubagentName): string {
  return path.join(SUBAGENTS_DIR, name, 'SYSTEM_PROMPT.md');
}

export function loadSystemPrompt(config: SubagentConfig): string {
  const p = config.systemPromptPath;
  if (!fs.existsSync(p)) {
    throw new Error(
      `subagent system prompt not found: ${p}\n` +
      `Run: yarn workspace @octocodeai/pi-extension build`,
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
      'chromeDebug',         // CDP execution — primary tool
      'web',                 // CDP docs + web research
      'localGetFileContent', // read source files, screenshots
      'localSearchCode',     // correlate browser errors to local source
      'localViewStructure',  // navigate file trees
    ],
    resourceMode: 'octocode' as ResourceMode,
    thinking: 'low',
    systemPromptPath: subagentPromptPath('browser-agent'),
    skills: [subagentSkillPath('browser-agent', 'browser-agent')],
  },
} satisfies Record<SubagentName, SubagentConfig>;

export const SUBAGENT_NAMES = Object.keys(SUBAGENT_REGISTRY) as SubagentName[];
