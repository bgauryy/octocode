import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { parseDocument } from 'yaml';
import { workspaceAgentRoot } from './paths.js';
import { capabilityDefinitionRevision, capabilitySourcePaths, stableCapabilitySourceId, repositoryCapabilityDirectories } from './capability-sources.js';
import type { CapabilitySourceStatus } from './capability-state.js';

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
  /** Explicit Pi runtime paths admit this file only, never its siblings. */
  readonly file?: string;
  readonly bundled?: boolean;
}

export interface AgentSkillInventoryEntry {
  readonly name: string;
  readonly source: string;
  readonly sourceId: string;
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
  readonly defaultEnabled: boolean;
  readonly bundled: boolean;
  readonly selected?: boolean;
  readonly status: CapabilitySourceStatus;
  readonly realPath?: string;
  readonly shadowedBy?: string;
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

export function discoverAgentSkillInventory(
  sources: readonly AgentSkillSourceDescriptor[],
  enablement?: AgentSkillEnablement,
  options: { trusted?: boolean } = {},
): AgentSkillInventoryResult {
  const entries: AgentSkillInventoryEntry[] = [];
  const errors: AgentSkillInventoryResult['errors'] = [];
  for (const source of [...sources].sort((a, b) => a.precedence - b.precedence)) {
    const seenDirectories = new Set<string>();
    const files: string[] = [];
    const visit = (dir: string): void => {
      try {
        const realDir = fs.realpathSync(dir);
        if (seenDirectories.has(realDir)) return;
        seenDirectories.add(realDir);
        const children = fs.readdirSync(dir, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name));
        if (children.some(child => child.name === 'SKILL.md')) files.push(path.join(dir, 'SKILL.md'));
        for (const child of children) {
          if (child.name === '.git' || child.name === 'node_modules') continue;
          const absolute = path.join(dir, child.name);
          try {
            if (child.isDirectory() || (child.isSymbolicLink() && fs.statSync(absolute).isDirectory())) visit(absolute);
          } catch (error) { errors.push({ path: absolute, error: (error as Error).message }); }
        }
      } catch (error) {
        if (fs.existsSync(dir)) errors.push({ path: dir, error: (error as Error).message });
      }
    };
    if (source.file) { if (fs.existsSync(source.file)) files.push(path.resolve(source.file)); }
    else visit(source.root);
    for (const skillPath of files) {
      const dir = path.dirname(skillPath);
      let raw: string | undefined;
      let revision: string | undefined;
      let realPath: string | undefined;
      const sourceId = stableCapabilitySourceId({ kind: 'skill', host: source.vendor, scope: source.scope, path: skillPath });
      const common = {
        source: source.id, sourceId, vendor: source.vendor, scope: source.scope,
        root: path.resolve(source.root), path: skillPath, precedence: source.precedence,
        bundled: source.bundled ?? source.id === 'pi:bundled', defaultEnabled: source.defaultEnabled,
      };
      const activation = (name: string) => {
        const trusted = source.scope !== 'workspace' || options.trusted !== false;
        const enabled = trusted && enabledFor(name, source, enablement);
        return { enabled, status: (trusted ? enabled ? 'active' : 'disabled' : 'untrusted') as CapabilitySourceStatus };
      };
      try {
        const linkStat = fs.lstatSync(skillPath);
        if (linkStat.isSymbolicLink()) throw new Error('SKILL.md symbolic links are not allowed');
        const realSkillPath = fs.realpathSync(skillPath);
        realPath = realSkillPath;
        const stat = fs.statSync(realSkillPath);
        if (!stat.isFile() || stat.size > MAX_SKILL_BYTES) throw new Error(`SKILL.md exceeds ${MAX_SKILL_BYTES} bytes`);
        raw = fs.readFileSync(realSkillPath, 'utf8');
        if (fs.realpathSync(skillPath) !== realSkillPath) throw new Error('Skill link target changed during discovery');
        const after = fs.statSync(realSkillPath);
        if (stat.dev !== after.dev || stat.ino !== after.ino || stat.size !== after.size || stat.mtimeMs !== after.mtimeMs) throw new Error('Skill definition changed during discovery');
        revision = capabilityDefinitionRevision({ raw, realPath });
        const parsed = parseAgentSkill(raw, path.basename(path.dirname(realSkillPath)));
        if (!parsed.ok) throw new Error(parsed.error);
        const skill = { ...parsed.skill, dir, path: skillPath };
        entries.push({
          name: skill.name,
          ...common,
          realPath,
          hash: revision,
          revision,
          parseStatus: 'valid',
          ...activation(skill.name),
          skill,
        });
      } catch (error) {
        const diagnostic = error instanceof Error ? error.message : 'Invalid skill';
        if (raw !== undefined && revision === undefined) revision = capabilityDefinitionRevision({ raw, realPath });
        entries.push({
          name: path.basename(dir),
          ...common,
          ...(revision ? { hash: revision } : {}),
          ...(revision ? { revision } : {}),
          parseStatus: 'invalid',
          diagnostic,
          ...activation(path.basename(dir)),
        });
        errors.push({ path: skillPath, error: diagnostic });
      }
    }
  }
  return { entries, errors };
}

export function resolveAgentSkillInventory(entries: readonly AgentSkillInventoryEntry[]): AgentSkillInventoryEntry[] {
  const winners = new Map<string, AgentSkillInventoryEntry>();
  const key = (name: string) => name.replace(/\s+/g, ' ').trim().toLowerCase();
  const active = (entry: AgentSkillInventoryEntry) => entry.enabled && entry.status === 'active';
  for (const entry of [...entries].sort((a, b) => Number(Boolean(a.selected)) - Number(Boolean(b.selected)) || Number(active(a)) - Number(active(b)) || Number(a.bundled) - Number(b.bundled) || Number(a.scope === 'workspace') - Number(b.scope === 'workspace') || a.precedence - b.precedence || a.sourceId.localeCompare(b.sourceId))) {
    // A selected pending/invalid/unavailable source reserves the name until explicitly replaced.
    if (entry.selected || ((active(entry) || entry.defaultEnabled) && entry.parseStatus === 'valid' && entry.skill)) winners.set(key(entry.name), entry);
  }
  return entries.map(entry => {
    const { shadowedBy: _shadowedBy, ...rest } = entry;
    const winner = winners.get(key(entry.name));
    return winner && winner !== entry ? { ...rest, shadowedBy: winner.sourceId } : rest;
  });
}

export function effectiveAgentSkills(entries: readonly AgentSkillInventoryEntry[]): AgentSkill[] {
  return resolveAgentSkillInventory(entries).filter(entry => !entry.shadowedBy && entry.enabled && entry.status === 'active' && entry.parseStatus === 'valid' && entry.skill)
    .map(entry => entry.skill!).sort((a, b) => a.name.localeCompare(b.name));
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

export function defaultAgentSkillSources(cwd: string, homeDir = os.homedir(), octocodeHome?: string, options: { env?: NodeJS.ProcessEnv; trusted?: boolean } = {}): AgentSkillSourceDescriptor[] {
  const paths = capabilitySourcePaths(cwd, { homeDir, octocodeHome, env: options.env });
  const nativeHome = paths.native.globalRoot;
  const relativeRoots: ReadonlyArray<{ relative: string; vendor: AgentSkillVendor }> = [
    { relative: '.claude/skills', vendor: 'claude' },
    { relative: '.cursor/skills', vendor: 'cursor' },
    { relative: '.codex/skills', vendor: 'codex' },
    { relative: '.agent/skills', vendor: 'agent' },
    { relative: '.agents/skills', vendor: 'agents' },
  ];
  const projectDirectories = repositoryDirectories(cwd);
  const candidates: Array<Omit<AgentSkillSourceDescriptor, 'id' | 'precedence'>> = [
    { vendor: 'octocode', scope: 'user', root: path.join(nativeHome, 'agent', 'skills'), defaultEnabled: true },
    ...relativeRoots.map(({ relative, vendor }) => ({ vendor, scope: 'user' as const, root: path.join(homeDir, relative), defaultEnabled: vendor === 'agents' })),
    { vendor: 'codex', scope: 'user', root: path.join(paths.codexHome, 'skills'), defaultEnabled: false },
    { vendor: 'octocode', scope: 'user', root: paths.native.skillsDir, defaultEnabled: true },
    ...projectDirectories.flatMap((directory) => [
      { vendor: 'octocode' as const, scope: 'workspace' as const, root: path.join(workspaceAgentRoot(directory, nativeHome), 'skills'), defaultEnabled: true },
      { vendor: 'octocode' as const, scope: 'workspace' as const, root: path.join(directory, '.octocode', 'skills'), defaultEnabled: true },
      ...relativeRoots.map(({ relative, vendor }) => ({ vendor, scope: 'workspace' as const, root: path.join(directory, relative), defaultEnabled: vendor === 'agents' })),
    ]),
  ];
  const seen = new Set<string>();
  return candidates.flatMap((candidate) => {
    const resolvedRoot = path.resolve(candidate.root);
    if (seen.has(resolvedRoot)) return [];
    seen.add(resolvedRoot);
    const precedence = seen.size - 1;
    return [{ ...candidate, defaultEnabled: candidate.defaultEnabled && (candidate.scope !== 'workspace' || options.trusted !== false), root: resolvedRoot, precedence, id: `${candidate.vendor}:${candidate.scope}:${resolvedRoot}` }];
  });
}

export function defaultAgentSkillRoots(cwd: string, homeDir = os.homedir(), octocodeHome?: string): string[] {
  return defaultAgentSkillSources(cwd, homeDir, octocodeHome).map(({ root }) => root);
}

export function repositoryDirectories(cwd: string): string[] {
  return repositoryCapabilityDirectories(cwd);
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
  readMcpConfigText,
  type DiscoverMcpConfigOptions,
  type DiscoveredMcpConfig,
  type DiscoveredMcpDefinition,
  type DiscoveredMcpServerConfig,
  type McpDiscoveryResult,
} from './mcp-discovery.js';
