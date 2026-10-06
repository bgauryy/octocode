import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { parseFrontmatter } from '@earendil-works/pi-coding-agent';
import { globalPaths, piAgentDir, projectAgentDirs } from '../shared/home.js';
import { packageRoot } from '../shared/package.js';

export interface AgentProfile {
  name: string;
  description: string;
  prompt: string;
  tools?: string;
  /** Comma-separated tools the child must not have (Pi's --exclude-tools), e.g. `file` for read-only profiles. */
  excludeTools?: string;
  model?: string;
  /** When set, the child's `browser` tool opens a visible Chrome on the persistent profile (logins stay). */
  visibleBrowser?: boolean;
  /** False: the child starts no MCP servers (their tool schemas would be paid on every request for nothing). */
  mcp?: false;
  /** True: the child runs the user's command hooks (`OCTOCODE_HOOKS`); by default subagents do not. */
  hooks?: true;
}

const BUNDLED_DIR = path.join(packageRoot(), 'subagents');

/**
 * Profiles by name, later directories overriding earlier: bundled → ~/.pi/agent/agents → ~/.octocode/agents →
 * <repo root>/.pi/agents → <repo root>/.octocode/agents. Project profiles (prompts, tools, models) load only when Pi
 * trusts the project.
 */
export function loadProfiles(cwd: string, home = os.homedir(), projectTrusted = true, env: NodeJS.ProcessEnv = process.env): Map<string, AgentProfile> {
  const profiles = new Map<string, AgentProfile>();
  const dirs = [BUNDLED_DIR, path.join(piAgentDir(home), 'agents'), globalPaths(env, home).agents, ...(projectTrusted ? projectAgentDirs(cwd) : [])];
  for (const dir of dirs) {
    let files: string[];
    try {
      files = fs.readdirSync(dir).filter((file) => file.endsWith('.md'));
    } catch {
      continue;
    }
    for (const file of files) {
      const profile = parseProfile(path.basename(file, '.md'), fs.readFileSync(path.join(dir, file), 'utf8'));
      profiles.set(profile.name, profile);
    }
  }
  return profiles;
}

export function parseProfile(fallbackName: string, text: string): AgentProfile {
  const { frontmatter, body } = parseFrontmatter<Record<string, unknown>>(text);
  const field = (key: string) => (typeof frontmatter[key] === 'string' ? (frontmatter[key] as string).trim() : undefined);
  const tools = field('tools');
  const excludeTools = field('excludeTools');
  const model = field('model');
  const visibleBrowser = frontmatter['visibleBrowser'];
  const mcp = frontmatter['mcp'];
  const hooks = frontmatter['hooks'];
  return {
    name: field('name') ?? fallbackName,
    description: field('description') ?? '',
    prompt: body.trim(),
    ...(tools ? { tools } : {}),
    ...(excludeTools ? { excludeTools } : {}),
    ...(model ? { model } : {}),
    ...(visibleBrowser === true || visibleBrowser === 'true' || visibleBrowser === '1' ? { visibleBrowser: true } : {}),
    ...(mcp === false || mcp === 'false' || mcp === '0' ? { mcp: false as const } : {}),
    ...(hooks === true || hooks === 'true' || hooks === '1' ? { hooks: true as const } : {}),
  };
}
