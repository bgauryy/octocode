/**
 * Subagent registry — typed configuration for every specialised Pi subagent
 * this extension ships.
 *
 * Each subagent has:
 *   - A typed name (union literal)
 *   - Tool allowlist (no nested spawning; role prompts bound repository edits and shell use)
 *   - Resource mode (always 'octocode' so the extension's own tools are available)
 *   - SYSTEM_PROMPT.md path loaded at runtime from dist/subagents/<name>/
 *   - Focused role skills, plus any subagent-local skill dirs
 *
 * The agent tool reads this registry and starts a worker through spawnRpcAgent.
 * All profiles share the same lifecycle registry for inspect, message, and wait.
 */

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { SUBAGENT_PLACEHOLDERS } from '@octocodeai/agent-contracts/prompts';
import type { ResourceMode } from './tools/agents/types.js';
import { discoverSkills } from './tools/skill-discovery.js';

// ─── Types ────────────────────────────────────────────────────────────────────

export interface SubagentConfig {
  /** Role identifier selected by the agent tool's profile mapping. */
  name: SubagentName;
  /** Human label shown in agent inspection output. */
  label: string;
  /** One-line description of what this subagent does. */
  description: string;
  /**
   * Requested tools; the effective parent grant bounds availability and excludes
   * recursive agent and tool/skill-authoring tools.
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
  /** Default Pi provider override. Set when a subagent's default model lives on a custom
   *  provider whose id collides with a builtin namespace (e.g. claude-*), so pi resolves
   *  --model to the right provider without the caller passing --provider each time. */
  provider?: string;
  /**
   * Absolute path to SYSTEM_PROMPT.md for this subagent.
   * Loaded at runtime from dist/subagents/<name>/SYSTEM_PROMPT.md.
   */
  systemPromptPath: string;
  /**
   * Static extra skill paths specific to this subagent (e.g. browser-agent's local
   * skill dir). Combined with focused enabled role skills at spawn time by
   * resolveSubagentSkills(). If `skills` is set explicitly, these are ignored.
   */
  extraSkillPaths?: string[];
  /** Focused role defaults. Other enabled skills require an explicit parent grant. */
  skillNames?: string[];
  /**
   * Explicit skill override. When set, resolveSubagentSkills returns it as-is.
   * If undefined (the normal case for SUBAGENT_REGISTRY entries), skills are
   * resolved lazily at spawn time from the canonical discovery owner.
   */
  skills?: string[];
}

/** Union of all registered subagent names (extend when adding new subagents). */
export type SubagentName =
  'browser-agent' | 'researcher' | 'planner' | 'architect' | 'implementer' | 'reviewer';

// ─── Runtime path resolution ──────────────────────────────────────────────────

function resolveSubagentsDir(): { dir: string; isBuilt: boolean } {
  const moduleDir = path.dirname(fileURLToPath(import.meta.url));
  const distDir = path.join(moduleDir, 'subagents');
  if (fs.existsSync(distDir)) return { dir: distDir, isBuilt: true };
  return { dir: path.resolve(moduleDir, '..', 'subagents'), isBuilt: false };
}

const _subagentsResolution = resolveSubagentsDir();
/** dist/subagents/ in published builds; packageRoot/subagents/ in source tests. */
const SUBAGENTS_DIR = _subagentsResolution.dir;
/** true when running from the built dist/ directory (placeholders are replaced). */
export const SUBAGENTS_IS_BUILT = _subagentsResolution.isBuilt;

function subagentSkillPath(name: SubagentName, skillName: string): string {
  return path.join(SUBAGENTS_DIR, name, 'skills', skillName);
}

/**
 * Resolves focused role skills at call time; never inherits the full inventory.
 *
 * - If config.skills is set explicitly, returns it as-is (override path).
 * - Otherwise runs canonical discovery and appends valid extraSkillPaths,
 *   so late-installed skills (added after process start) are discovered without restart.
 */
export function resolveSubagentSkills(
  config: SubagentConfig | { skills?: string[]; extraSkillPaths?: string[]; skillNames?: string[] },
  cwd = process.cwd(),
): string[] {
  if (config.skills !== undefined) return config.skills;
  const selectedNames = new Set(config.skillNames ?? []);
  const discovered = (selectedNames.size ? discoverSkills(cwd) : [])
    .filter(skill => selectedNames.has(skill.name))
    .map((skill) => skill.dir)
    .filter((dir) => Boolean(dir) && fs.existsSync(path.join(dir, 'SKILL.md')));
  const extras = (config.extraSkillPaths ?? []).filter((dir) => fs.existsSync(path.join(dir, 'SKILL.md')));
  return [...new Set([...discovered, ...extras])];
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
  const prompt = fs.readFileSync(p, 'utf8');
  // Guard only applies when loading from the built dist/ directory.
  // The source-tree subagents/ templates intentionally contain {{...}} placeholders
  // that are expanded at build time. In test/dev fallback mode (no dist/ present)
  // the placeholder check is skipped to keep unit tests runnable without a prior build.
  if (SUBAGENTS_IS_BUILT) {
    const leftover = SUBAGENT_PLACEHOLDERS.find((ph) => prompt.includes(ph));
    if (leftover) {
      throw new Error(
        `subagent ${path.basename(path.dirname(p))}: prompt contains unexpanded placeholder "${leftover}".\n` +
          `Run: yarn workspace @octocodeai/pi-extension build`
      );
    }
  }
  return prompt;
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
      'web',         // CDP docs + web research
      'MCPTool',     // Octocode MCP server: localFetch, localSearch, astSearch, etc.
      'file',        // only parent-assigned durable handback artifacts
      'skill',       // load bundled/user workflows, including Awareness
      'awareness',   // native coordination, memory, verification, and history gateway
      'bash',        // assigned shell work
    ],
    resourceMode: 'octocode' as ResourceMode,
    thinking: 'low',
    systemPromptPath: subagentPromptPath('browser-agent'),
    extraSkillPaths: [subagentSkillPath('browser-agent', 'browser-agent')],
    skillNames: ['octocode-chrome-devtools', 'octocode-awareness'],
  },
  researcher: {
    name: 'researcher' as SubagentName,
    label: 'Researcher',
    description:
      'Fast Octocode research specialist. Has web, GitHub, npm, local, binary, and LSP tools. ' +
      'Use for evidence gathering, prior art, package/repo lookup, and concise claim ledgers.',
    tools: [
      'web',
      'MCPTool', // octocode MCP server: all GitHub, local, LSP, npm research tools
      'file',   // only parent-assigned durable handback artifacts
      'skill',  // load bundled/user workflows, including Awareness
      'awareness', // native coordination, memory, verification, and history gateway
      'bash',   // assigned shell work
    ],
    resourceMode: 'octocode' as ResourceMode,
    thinking: 'low',
    systemPromptPath: subagentPromptPath('researcher'),
    skillNames: ['octocode-research', 'octocode-awareness'],
  },
  planner: {
    name: 'planner' as SubagentName,
    label: 'Planner',
    description:
      'Implementation planning specialist. Has focused research and planning workflows. ' +
      'Use for dependency-ordered plans, risks, verification strategy, and RFC handoff packets.',
    tools: [
      'web',
      'MCPTool', // octocode MCP server: all GitHub, local, LSP, npm research tools
      'file',   // only parent-assigned durable handback artifacts
      'skill',  // load bundled/user workflows, including Awareness
      'awareness', // native coordination, memory, verification, and history gateway
      'bash',   // assigned shell work
    ],
    resourceMode: 'octocode' as ResourceMode,
    thinking: 'low',
    systemPromptPath: subagentPromptPath('planner'),
    skillNames: ['octocode-research', 'octocode-rfc-generator', 'octocode-awareness'],
  },
  architect: {
    name: 'architect' as SubagentName,
    label: 'Architect',
    description:
      'Root-cause and local-code architecture specialist. Has focused research and architecture workflows, local/LSP tools, ' +
      'GitHub history, web, and bash for targeted debug/test loops.',
    tools: [
      'bash',
      'web',
      'MCPTool', // octocode MCP server: all GitHub, local, LSP, npm research tools
      'file',   // only parent-assigned durable handback artifacts
      'skill',  // load bundled/user workflows, including Awareness
      'awareness', // native coordination, memory, verification, and history gateway
    ],
    resourceMode: 'octocode' as ResourceMode,
    thinking: 'medium',
    systemPromptPath: subagentPromptPath('architect'),
    skillNames: ['octocode-research', 'octocode-architect', 'octocode-awareness'],
  },
  implementer: {
    name: 'implementer' as SubagentName,
    label: 'Implementer',
    description:
      'Bounded code implementation specialist. Edits only explicitly assigned ownership, runs the assigned acceptance check, and returns evidence for parent verification.',
    tools: [
      'MCPTool', // inspect exact code, symbols, callers, and contracts
      'file',    // assigned source edits and durable handback
      'skill',   // load only a matching implementation workflow
      'awareness', // native coordination, verification, and handoff gateway
      'bash',    // assigned tests/builds
    ],
    resourceMode: 'octocode' as ResourceMode,
    thinking: 'medium',
    systemPromptPath: subagentPromptPath('implementer'),
    skillNames: ['octocode-research', 'octocode-awareness'],
  },
  reviewer: {
    name: 'reviewer' as SubagentName,
    label: 'Reviewer',
    description:
      'Read-only acceptance reviewer. Rechecks the assigned claim against exact evidence and returns a typed PASS, WARN, or FAIL verdict; the parent keeps completion authority.',
    tools: [
      'MCPTool',
      'skill',
      'awareness',
    ],
    resourceMode: 'octocode' as ResourceMode,
    thinking: 'medium',
    systemPromptPath: subagentPromptPath('reviewer'),
    skillNames: ['octocode-research', 'octocode-awareness'],
  },
} satisfies Record<SubagentName, SubagentConfig>;
