import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { parseDocument } from 'yaml';
import { getOctocodeHome } from '@octocodeai/config';
import { workspaceAgentRoot } from './paths.js';

const NAME_RE = /^(?!-)(?!.*--)[a-z0-9-]{1,64}(?<!-)$/;
const MAX_DESCRIPTION = 1_024;
const MAX_COMPATIBILITY = 500;
const MAX_SKILL_BYTES = 512 * 1024;

export interface AgentSkillMetadata {
  name: string;
  description: string;
  license?: string;
  compatibility?: string;
  metadata?: Record<string, string>;
  /** Experimental Agent Skills field. It is descriptive and never grants runtime permission. */
  allowedTools?: string;
}

export interface AgentSkill extends AgentSkillMetadata {
  dir: string;
  path: string;
  body: string;
  source: string;
}

export type AgentSkillParseResult =
  | { ok: true; skill: Omit<AgentSkill, 'dir' | 'path'> }
  | { ok: false; error: string };

function stringField(record: Record<string, unknown>, name: string): string | undefined {
  const value = record[name];
  return typeof value === 'string' ? value.trim() : undefined;
}

export function parseAgentSkill(source: string, directoryName?: string): AgentSkillParseResult {
  if (!source.startsWith('---\n') && !source.startsWith('---\r\n')) return { ok: false, error: 'SKILL.md must start with YAML frontmatter' };
  const match = source.match(/^---\r?\n([\s\S]*?)\r?\n---(?:\r?\n|$)/);
  if (!match) return { ok: false, error: 'SKILL.md frontmatter is not closed' };
  const document = parseDocument(match[1]!, { uniqueKeys: true });
  if (document.errors.length > 0) return { ok: false, error: `Invalid YAML frontmatter: ${document.errors[0]!.message}` };
  const raw = document.toJS({ maxAliasCount: 0 }) as unknown;
  if (typeof raw !== 'object' || raw === null || Array.isArray(raw)) return { ok: false, error: 'Skill frontmatter must be a mapping' };
  const record = raw as Record<string, unknown>;
  const name = stringField(record, 'name');
  const description = stringField(record, 'description');
  if (!name || !NAME_RE.test(name)) return { ok: false, error: 'Skill name must match the Agent Skills naming rules' };
  if (directoryName !== undefined && name !== directoryName) return { ok: false, error: `Skill name ${name} must match parent directory ${directoryName}` };
  if (!description || description.length > MAX_DESCRIPTION) return { ok: false, error: 'Skill description must contain 1-1024 characters' };
  const license = record.license === undefined ? undefined : stringField(record, 'license');
  if (record.license !== undefined && !license) return { ok: false, error: 'Skill license must be a non-empty string' };
  const compatibility = record.compatibility === undefined ? undefined : stringField(record, 'compatibility');
  if (record.compatibility !== undefined && (!compatibility || compatibility.length > MAX_COMPATIBILITY)) {
    return { ok: false, error: 'Skill compatibility must contain 1-500 characters' };
  }
  let metadata: Record<string, string> | undefined;
  if (record.metadata !== undefined) {
    if (typeof record.metadata !== 'object' || record.metadata === null || Array.isArray(record.metadata)) return { ok: false, error: 'Skill metadata must be a mapping' };
    const entries = Object.entries(record.metadata as Record<string, unknown>);
    if (entries.some(([key, value]) => !key || typeof value !== 'string')) return { ok: false, error: 'Skill metadata keys and values must be strings' };
    metadata = Object.fromEntries(entries) as Record<string, string>;
  }
  const allowedTools = record['allowed-tools'] === undefined ? undefined : stringField(record, 'allowed-tools');
  if (record['allowed-tools'] !== undefined && !allowedTools) return { ok: false, error: 'Skill allowed-tools must be a non-empty string' };
  const body = source.slice(match[0].length);
  return {
    ok: true,
    skill: {
      name,
      description,
      ...(license ? { license } : {}),
      ...(compatibility ? { compatibility } : {}),
      ...(metadata ? { metadata } : {}),
      ...(allowedTools ? { allowedTools } : {}),
      body,
      source,
    },
  };
}

export interface AgentSkillDiscoveryResult {
  skills: AgentSkill[];
  errors: Array<{ path: string; error: string }>;
}

export type AgentSkillVendor = 'octocode' | 'pi' | 'agent' | 'agents' | 'claude' | 'cursor' | 'codex' | 'custom';
export type AgentSkillScope = 'user' | 'workspace';

export interface AgentSkillSourceDescriptor {
  readonly id: string;
  readonly vendor: AgentSkillVendor;
  readonly scope: AgentSkillScope;
  readonly root: string;
  readonly precedence: number;
  readonly defaultEnabled: boolean;
}

export interface AgentSkillInventoryEntry {
  readonly name: string;
  readonly source: string;
  readonly vendor: AgentSkillVendor;
  readonly scope: AgentSkillScope;
  readonly root: string;
  readonly path: string;
  readonly precedence: number;
  readonly hash?: string;
  readonly revision?: string;
  readonly parseStatus: 'valid' | 'invalid';
  readonly diagnostic?: string;
  readonly enabled: boolean;
  readonly skill?: AgentSkill;
}

export interface AgentSkillInventoryResult {
  readonly entries: AgentSkillInventoryEntry[];
  readonly errors: Array<{ path: string; error: string }>;
}

export type AgentSkillEnablement = (name: string, source: AgentSkillSourceDescriptor) => boolean | undefined;

function enabledFor(name: string, source: AgentSkillSourceDescriptor, override?: AgentSkillEnablement): boolean {
  return override?.(name, source) ?? source.defaultEnabled;
}

function isContainedBy(root: string, candidate: string): boolean {
  const relative = path.relative(root, candidate);
  return relative === '' || (!path.isAbsolute(relative) && relative !== '..' && !relative.startsWith(`..${path.sep}`));
}

export function discoverAgentSkillInventory(
  sources: readonly AgentSkillSourceDescriptor[],
  enablement?: AgentSkillEnablement,
): AgentSkillInventoryResult {
  const entries: AgentSkillInventoryEntry[] = [];
  const errors: AgentSkillInventoryResult['errors'] = [];
  for (const source of [...sources].sort((a, b) => a.precedence - b.precedence)) {
    let directories: fs.Dirent[];
    try { directories = fs.readdirSync(source.root, { withFileTypes: true }); }
    catch { continue; }
    for (const directory of directories.sort((a, b) => a.name.localeCompare(b.name))) {
      if (!directory.isDirectory() || directory.isSymbolicLink()) continue;
      const dir = path.join(source.root, directory.name);
      const skillPath = path.join(dir, 'SKILL.md');
      if (!fs.existsSync(skillPath)) continue;
      let raw: string | undefined;
      let revision: string | undefined;
      try {
        const linkStat = fs.lstatSync(skillPath);
        if (linkStat.isSymbolicLink()) throw new Error('SKILL.md symbolic links are not allowed');
        const realRoot = fs.realpathSync(source.root);
        const realDir = fs.realpathSync(dir);
        const realSkillPath = fs.realpathSync(skillPath);
        if (!isContainedBy(realRoot, realDir) || !isContainedBy(realDir, realSkillPath)) {
          throw new Error('SKILL.md resolves outside its discovery root');
        }
        const stat = fs.statSync(realSkillPath);
        if (!stat.isFile() || stat.size > MAX_SKILL_BYTES) throw new Error(`SKILL.md exceeds ${MAX_SKILL_BYTES} bytes`);
        raw = fs.readFileSync(realSkillPath, 'utf8');
        revision = `sha256:${createHash('sha256').update(raw).digest('hex')}`;
        const parsed = parseAgentSkill(raw, directory.name);
        if (!parsed.ok) throw new Error(parsed.error);
        const skill = { ...parsed.skill, dir, path: skillPath };
        entries.push({
          name: skill.name,
          source: source.id,
          vendor: source.vendor,
          scope: source.scope,
          root: path.resolve(source.root),
          path: skillPath,
          precedence: source.precedence,
          hash: revision,
          revision,
          parseStatus: 'valid',
          enabled: enabledFor(skill.name, source, enablement),
          skill,
        });
      } catch (error) {
        const diagnostic = error instanceof Error ? error.message : 'Invalid skill';
        if (raw !== undefined && revision === undefined) revision = `sha256:${createHash('sha256').update(raw).digest('hex')}`;
        entries.push({
          name: directory.name,
          source: source.id,
          vendor: source.vendor,
          scope: source.scope,
          root: path.resolve(source.root),
          path: skillPath,
          precedence: source.precedence,
          ...(revision ? { hash: revision } : {}),
          ...(revision ? { revision } : {}),
          parseStatus: 'invalid',
          diagnostic,
          enabled: enabledFor(directory.name, source, enablement),
        });
        errors.push({ path: skillPath, error: diagnostic });
      }
    }
  }
  return { entries, errors };
}

export function effectiveAgentSkills(entries: readonly AgentSkillInventoryEntry[]): AgentSkill[] {
  const effective = new Map<string, AgentSkill>();
  for (const entry of [...entries].sort((a, b) => a.precedence - b.precedence)) {
    if (entry.enabled && entry.parseStatus === 'valid' && entry.skill) effective.set(entry.name, entry.skill);
  }
  return [...effective.values()].sort((a, b) => a.name.localeCompare(b.name));
}

export function discoverAgentSkills(roots: readonly string[]): AgentSkillDiscoveryResult {
  const inventory = discoverAgentSkillInventory(roots.map((root, precedence) => ({
    id: `custom:${precedence}:${path.resolve(root)}`,
    vendor: 'custom',
    scope: 'user',
    root,
    precedence,
    defaultEnabled: true,
  })));
  return { skills: effectiveAgentSkills(inventory.entries), errors: inventory.errors };
}

export function defaultAgentSkillSources(cwd: string, homeDir = os.homedir(), octocodeHome = getOctocodeHome()): AgentSkillSourceDescriptor[] {
  const relativeRoots: ReadonlyArray<{ relative: string; vendor: AgentSkillVendor }> = [
    { relative: '.pi/skills', vendor: 'pi' },
    { relative: '.pi/agent/skills', vendor: 'pi' },
    { relative: '.claude/skills', vendor: 'claude' },
    { relative: '.cursor/skills', vendor: 'cursor' },
    { relative: '.codex/skills', vendor: 'codex' },
    { relative: '.agent/skills', vendor: 'agent' },
    { relative: '.agents/skills', vendor: 'agents' },
  ];
  const projectDirectories = repositoryDirectories(cwd);
  const candidates: Array<Omit<AgentSkillSourceDescriptor, 'id' | 'precedence'>> = [
    ...relativeRoots.map(({ relative, vendor }) => ({ vendor, scope: 'user' as const, root: path.join(homeDir, relative), defaultEnabled: false })),
    { vendor: 'octocode', scope: 'user', root: path.join(octocodeHome, 'agent', 'skills'), defaultEnabled: true },
    ...projectDirectories.flatMap((directory) => [
      ...relativeRoots.map(({ relative, vendor }) => ({ vendor, scope: 'workspace' as const, root: path.join(directory, relative), defaultEnabled: false })),
      { vendor: 'octocode' as const, scope: 'workspace' as const, root: path.join(workspaceAgentRoot(directory, octocodeHome), 'skills'), defaultEnabled: true },
    ]),
  ];
  const seen = new Set<string>();
  return candidates.flatMap((candidate) => {
    const resolvedRoot = path.resolve(candidate.root);
    if (seen.has(resolvedRoot)) return [];
    seen.add(resolvedRoot);
    const precedence = seen.size - 1;
    return [{ ...candidate, root: resolvedRoot, precedence, id: `${candidate.vendor}:${candidate.scope}:${resolvedRoot}` }];
  });
}

export function defaultAgentSkillRoots(cwd: string, homeDir = os.homedir(), octocodeHome = getOctocodeHome()): string[] {
  return defaultAgentSkillSources(cwd, homeDir, octocodeHome).map(({ root }) => root);
}

export function repositoryDirectories(cwd: string): string[] {
  const resolved = path.resolve(cwd);
  const descending = [resolved];
  let current = resolved;
  for (;;) {
    if (fs.existsSync(path.join(current, '.git'))) return descending.reverse();
    const parent = path.dirname(current);
    if (parent === current) return [resolved];
    descending.push(parent);
    current = parent;
  }
}

export function listAgentSkillFiles(skillDir: string, maxDepth = 2, maxFiles = 30): string[] {
  const files: string[] = [];
  const visit = (dir: string, depth: number): void => {
    if (depth > maxDepth || files.length >= maxFiles) return;
    for (const entry of fs.readdirSync(dir, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))) {
      if (files.length >= maxFiles) break;
      if (entry.isSymbolicLink()) continue;
      const absolute = path.join(dir, entry.name);
      if (entry.isDirectory()) visit(absolute, depth + 1);
      else if (entry.isFile() && absolute !== path.join(skillDir, 'SKILL.md')) files.push(path.relative(skillDir, absolute));
    }
  };
  visit(skillDir, 0);
  return files;
}

// Agent Skills and MCP are both host-neutral capability discovery inputs.
export {
  discoverMcpConfigs,
  discoverMcpSystem,
  type DiscoverMcpConfigOptions,
  type DiscoveredMcpConfig,
  type DiscoveredMcpDefinition,
  type DiscoveredMcpServerConfig,
  type McpDiscoveryResult,
} from './mcp-discovery.js';
