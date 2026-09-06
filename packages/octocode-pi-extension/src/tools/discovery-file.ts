import { extensionWorkspaceRoot } from '../extension-paths.js';
/**
 * Discovery file — one machine-readable inventory of everything this session
 * can do: every discovered Agent Skill (across the common ecosystem roots,
 * deduped by name), the full MCP configuration (active sources, servers,
 * discovered tools, plus every MCP config file found in common claude/cursor/
 * codex/octocode/agent locations), and the native tool surface. Foreign MCP
 * definitions are available to the manager but remain disabled by default.
 *
 * Written to `.octocode/discovery.json` at session start (after init MCP
 * discovery lands) so users, peer agents, and external tooling can discover
 * the harness surface from one file instead of spelunking prompts and configs.
 * Best-effort: a failed write never affects the session.
 */

import fs from 'node:fs';
import path from 'node:path';
import type { PiContext } from '../types.js';
import { getMcpDiscoverySnapshot, type McpDiscoverySnapshot } from './mcp-tool.js';
import {
  discoverMcpConfigs,
  type DiscoveredMcpConfig,
} from './mcp-discovery.js';
import type { DiscoveredSkillState } from './skill-tool.js';


/** Per-section character counts for the harness prompt overhead. */
export interface SystemPromptStats {
  /** Octocode system prompt text (prompt.ts + Pi's own prompt). */
  sysChars: number;
  /** MCP catalog addendum chars (all server instructions + tool schemas). */
  mcpChars: number;
  /**
   * Dynamic addenda chars: available-skills block + active-plan block +
   * dynamic-capabilities addendum (changes every turn).
   */
  dynamicChars: number;
  /** Total of all three sections. */
  totalChars: number;
  /** Provider-visible direct tool descriptions plus their JSON schemas. */
  directToolChars: number;
  /** Initial provider subtotal: system prompt plus direct tool contracts. */
  providerSubtotalChars: number;
  /** Rough token estimate of the provider subtotal at 4 chars/token. */
  estimatedTokens: number;
  mcpServers: number;
  mcpTools: number;
  skills: number;
  status: 'pending' | 'frozen' | 'stale';
  mode: 'exact' | 'compact';
}

export interface DiscoverySnapshot {
  version: 1;
  generatedAt: string;
  workspace: string;
  harness: string;
  /** System prompt overhead snapshot from the last before_agent_start (or session_start). */
  systemPromptStats?: SystemPromptStats;
  /** Model-callable native tool names registered by the extension. */
  nativeTools: string[];
  nativeToolCount: number;
  /** Complete discovered inventory. Disabled entries remain visible but are not model-callable. */
  skills: Array<{ name: string; description: string; source: string; path: string; enabled: boolean }>;
  mcp: McpDiscoverySnapshot & { discoveredConfigs: DiscoveredMcpConfig[] };
}

export function getDiscoveryFilePath(cwd: string): string {
  return path.join(extensionWorkspaceRoot(cwd), 'discovery.json');
}

export async function buildDiscoverySnapshot(
  ctx: PiContext | undefined,
  opts: {
    skills: DiscoveredSkillState[];
    nativeTools: string[];
    home?: string;
    octocodeHome?: string;
    overhead?: {
      sysChars: number; mcpChars: number; dynamicChars: number;
      totalChars: number; mcpServers: number; mcpTools: number; skills: number;
      directToolChars?: number; status?: 'pending' | 'frozen' | 'stale'; mode?: 'exact' | 'compact';
    };
  },
): Promise<DiscoverySnapshot> {
  const workspace = ctx?.cwd ?? process.cwd();
  const sortedTools = [...opts.nativeTools].sort((a, b) => a.localeCompare(b));
  const systemPromptStats: SystemPromptStats | undefined = opts.overhead
    ? {
        sysChars: opts.overhead.sysChars,
        mcpChars: opts.overhead.mcpChars,
        dynamicChars: opts.overhead.dynamicChars,
        totalChars: opts.overhead.totalChars,
        directToolChars: opts.overhead.directToolChars ?? 0,
        providerSubtotalChars: opts.overhead.totalChars + (opts.overhead.directToolChars ?? 0),
        estimatedTokens: Math.round((opts.overhead.totalChars + (opts.overhead.directToolChars ?? 0)) / 4),
        mcpServers: opts.overhead.mcpServers,
        mcpTools: opts.overhead.mcpTools,
        skills: opts.overhead.skills,
        status: opts.overhead.status ?? 'pending',
        mode: opts.overhead.mode ?? 'exact',
      }
    : undefined;
  return {
    version: 1,
    generatedAt: new Date().toISOString(),
    workspace,
    harness: '@octocodeai/pi-extension',
    ...(systemPromptStats ? { systemPromptStats } : {}),
    nativeTools: sortedTools,
    nativeToolCount: sortedTools.length,
    skills: opts.skills.map((skill) => ({
      name: skill.name,
      description: skill.description,
      source: skill.source,
      path: skill.path,
      enabled: skill.enabled,
    })),
    mcp: {
      ...(await getMcpDiscoverySnapshot(ctx)),
      discoveredConfigs: discoverMcpConfigs(workspace, { homeDir: opts.home, octocodeHome: opts.octocodeHome }),
    },
  };
}

/**
 * Write the discovery inventory atomically. Returns the file path, or null when
 * the write failed (never throws — discovery is observability, not a dependency).
 */
export async function writeDiscoveryFile(
  ctx: PiContext | undefined,
  opts: {
    skills: DiscoveredSkillState[];
    nativeTools: string[];
    home?: string;
    octocodeHome?: string;
    overhead?: { sysChars: number; mcpChars: number; dynamicChars: number;
                 totalChars: number; mcpServers: number; mcpTools: number; skills: number;
                 directToolChars?: number; status?: 'pending' | 'frozen' | 'stale'; mode?: 'exact' | 'compact' };
  },
): Promise<string | null> {
  try {
    const snapshot = await buildDiscoverySnapshot(ctx, opts);
    const filePath = getDiscoveryFilePath(snapshot.workspace);
    fs.mkdirSync(path.dirname(filePath), { recursive: true });
    const tmp = `${filePath}.${process.pid}.tmp`;
    fs.writeFileSync(tmp, JSON.stringify(snapshot, null, 2) + '\n', 'utf8');
    fs.renameSync(tmp, filePath);
    return filePath;
  } catch {
    return null;
  }
}
