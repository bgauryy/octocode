import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { defaultAgentSkillSources, discoverAgentSkillInventory, resolveAgentSkillInventory, type AgentSkillInventoryEntry, type AgentSkillSourceDescriptor } from '../contracts/agent-skills.js';
import { capabilitySourcePaths, repositoryCapabilityDirectories, capabilityDefinitionRevision, stableCapabilitySourceId, type CapabilityPathOptions } from '../contracts/capability-sources.js';
import { getCapabilitySourceReview, getCapabilitySourceStatus, getSelectedCapabilitySource, listCapabilitySourceReviews, reviewCapabilitySource, type CapabilitySourceStatus } from '../contracts/capability-state.js';
import { getSkillEnablement } from '../contracts/mcp-state.js';
import type { ReadableSqlite } from '@octocodeai/octocode-awareness/host';
import type { SkillInfo } from '../types.js';
import { getAssetPaths } from '../assets.js';
import { extensionStateDbPath } from '../extension-paths.js';
import { openOctocodeDb } from './storage-policy.js';

export interface DiscoveredSkill {
  name: string;
  description: string;
  path: string;
  dir: string;
  source: string;
  sourceId?: string;
  revision?: string;
}
export interface DiscoveredSkillState extends DiscoveredSkill { enabled: boolean }
export interface DiscoveredSkillCandidate extends DiscoveredSkillState {
  sourceId: string;
  status: CapabilitySourceStatus;
  selected: boolean;
  shadowedBy?: string;
  parseStatus: AgentSkillInventoryEntry['parseStatus'];
  diagnostic?: string;
  vendor: AgentSkillInventoryEntry['vendor'];
  scope: AgentSkillInventoryEntry['scope'];
  root: string;
  defaultEnabled: boolean;
  bundled: boolean;
}
export interface SkillDiscoveryOptions extends CapabilityPathOptions {
  trusted?: boolean;
  db?: ReadableSqlite;
  bundledDir?: string;
}

function discoveryDb(options: SkillDiscoveryOptions): ReadableSqlite | undefined {
  if (options.db) return options.db;
  // Inventory reads do not create state simply because a source directory exists.
  try { return fs.existsSync(extensionStateDbPath()) ? openOctocodeDb() : undefined; } catch { return undefined; }
}

export function skillDiscoveryRoots(cwd: string, home = os.homedir()): Array<{ dir: string; source: string }> {
  return defaultAgentSkillSources(cwd, home).map(source => ({ dir: source.root, source: source.id }));
}

function pathIdentities(file: string): string[] {
  const resolved = path.resolve(file);
  try { return [...new Set([resolved, fs.realpathSync(resolved)])]; }
  catch { return [resolved]; }
}

/** Host metadata cannot turn a configured source into a second, enabled source. */
function hasDiscoveryOwner(file: string, sources: Array<{ exact: boolean; paths: string[] }>): boolean {
  const identities = pathIdentities(file);
  return sources.some(source => source.paths.some(root =>
    identities.some(candidate => {
      if (source.exact) return candidate === root;
      const relative = path.relative(root, candidate);
      return relative === '' || (relative !== '..' && !relative.startsWith(`..${path.sep}`) && !path.isAbsolute(relative));
    })));
}

/** Full source alternatives are retained; only the shared resolver chooses winners. */
export function discoverSkillCandidates(cwd: string, piSkills?: SkillInfo[], home = os.homedir(), options: SkillDiscoveryOptions = {}): DiscoveredSkillCandidate[] {
  const sources: AgentSkillSourceDescriptor[] = defaultAgentSkillSources(cwd, options.homeDir ?? home, options.octocodeHome, options);
  try {
    sources.push({ id: 'pi:bundled', vendor: 'pi', scope: 'user', root: options.bundledDir ?? getAssetPaths().skillsDir, precedence: -1, defaultEnabled: true, bundled: true });
  } catch (error) { console.warn('[octocode:skills] bundled skill directory unavailable:', (error as Error).message); }
  const metadata: AgentSkillInventoryEntry[] = [];
  const paths = capabilitySourcePaths(cwd, { ...options, homeDir: options.homeDir ?? home });
  const piDefaults = [paths.pi.agentDir, path.join(paths.homeDir, '.pi'), ...repositoryCapabilityDirectories(cwd).map(directory => path.join(directory, '.pi'))];
  const discoveryOwners = sources.map(source => ({ exact: Boolean(source.file), paths: pathIdentities(source.file ?? source.root) }));
  for (const skill of piSkills ?? []) {
    const name = skill.name?.trim();
    if (!name) continue;
    const file = (skill as { path?: string; filePath?: string }).path ?? (skill as { path?: string; filePath?: string }).filePath;
    const scope = skill.scope === 'project' || skill.scope === 'workspace' ? 'workspace' : 'user';
    if (file) {
      const resolved = path.resolve(file);
      // Host metadata must not reintroduce Pi defaults into Octocode's skill catalog.
      if (piDefaults.some(root => { const relative = path.relative(root, resolved); return relative === '' || (!relative.startsWith(`..${path.sep}`) && relative !== '..' && !path.isAbsolute(relative)); })) continue;
      if (hasDiscoveryOwner(resolved, discoveryOwners)) continue;
      sources.push({ id: `pi:runtime:${resolved}`, vendor: 'pi', scope, root: path.dirname(resolved), file: resolved, precedence: sources.length, defaultEnabled: true });
      discoveryOwners.push({ exact: true, paths: pathIdentities(resolved) });
      continue;
    }
    const sourceId = stableCapabilitySourceId({ kind: 'skill', host: 'pi', scope, path: path.join(cwd, '.pi', 'metadata', name), name });
    metadata.push({ name, source: 'pi:metadata', sourceId, vendor: 'pi', scope, root: '', path: '', precedence: sources.length, revision: capabilityDefinitionRevision(skill), defaultEnabled: false, bundled: false, parseStatus: 'invalid', enabled: false, status: 'unavailable', diagnostic: 'Pi skill metadata has no concrete skill file' });
  }
  const db = discoveryDb(options);
  const scopeKey = path.resolve(cwd);
  const inventory = discoverAgentSkillInventory(sources, undefined, { trusted: options.trusted });
  const entries: AgentSkillInventoryEntry[] = [...metadata, ...inventory.entries].map(entry => {
    if (!db) return entry;
    const reviewed = getCapabilitySourceReview(db, scopeKey, entry.sourceId);
    const selected = getSelectedCapabilitySource(db, scopeKey, 'skill', entry.name) === entry.sourceId && Boolean(reviewed);
    if (!entry.revision || entry.parseStatus !== 'valid') {
      const status = reviewed ? entry.scope === 'workspace' && options.trusted === false ? 'untrusted' : entry.revision ? 'pending-review' : 'unavailable' : entry.status;
      return { ...entry, enabled: false, selected, status };
    }
    const status = getCapabilitySourceStatus(db, scopeKey, { kind: 'skill', sourceId: entry.sourceId, revision: entry.revision, name: entry.name, path: entry.path || `pi:${entry.name}`, trusted: entry.scope !== 'workspace' || options.trusted !== false, defaultEnabled: entry.defaultEnabled });
    const enabled = status === 'active' && getSkillEnablement(db, scopeKey, entry.name, true, entry.sourceId);
    return { ...entry, enabled, status: enabled || status !== 'active' ? status : 'disabled' as const, selected };
  });
  if (db) {
    const available = new Set(entries.map(entry => entry.sourceId));
    for (const review of listCapabilitySourceReviews(db, scopeKey)) {
      if (review.kind !== 'skill' || available.has(review.sourceId)) continue;
      entries.push({ name: review.name, path: review.path, source: 'linked', sourceId: review.sourceId, revision: review.revision, enabled: false, status: 'unavailable', selected: getSelectedCapabilitySource(db, scopeKey, 'skill', review.name) === review.sourceId, parseStatus: 'invalid', diagnostic: 'Reviewed skill source is unavailable', vendor: 'custom', scope: review.scopeKey === '*' ? 'user' : 'workspace', root: path.dirname(review.path), precedence: 0, defaultEnabled: false, bundled: false });
    }
  }
  const candidates: DiscoveredSkillCandidate[] = resolveAgentSkillInventory(entries).map(entry => ({
    name: entry.name, description: entry.skill?.description ?? '', path: entry.path, dir: entry.skill?.dir ?? path.dirname(entry.path),
    source: entry.bundled ? 'bundled' : entry.source === 'linked' ? 'linked' : entry.source === 'pi:metadata' ? 'pi' : entry.scope === 'workspace' ? entry.vendor === 'agents' ? 'project' : `project:${entry.vendor}` : entry.vendor === 'pi' ? 'user' : `user:${entry.vendor}`,
    sourceId: entry.sourceId, revision: entry.revision, enabled: entry.enabled, status: entry.status, selected: entry.selected ?? false,
    shadowedBy: entry.shadowedBy, parseStatus: entry.parseStatus, diagnostic: entry.diagnostic, vendor: entry.vendor, scope: entry.scope,
    root: entry.root, defaultEnabled: entry.defaultEnabled, bundled: entry.bundled,
  }));
  return candidates.sort((a, b) => a.name.localeCompare(b.name) || (a.sourceId ?? a.path).localeCompare(b.sourceId ?? b.path));
}

export function discoverAllSkills(cwd: string, piSkills?: SkillInfo[], home = os.homedir(), options: SkillDiscoveryOptions = {}): DiscoveredSkill[] {
  return discoverSkillStates(cwd, piSkills, home, options).map(({ enabled: _enabled, ...skill }) => skill);
}
export function discoverSkillStates(cwd: string, piSkills?: SkillInfo[], home = os.homedir(), options: SkillDiscoveryOptions = {}): DiscoveredSkillState[] {
  return discoverSkillCandidates(cwd, piSkills, home, options).filter(skill => !skill.shadowedBy && skill.parseStatus === 'valid' && (skill.enabled || skill.defaultEnabled || skill.selected))
    .map(({ name, description, path: filePath, dir, source, sourceId, revision, enabled }) => ({ name, description, path: filePath, dir, source, sourceId, revision, enabled }));
}
export function discoverSkills(cwd: string, piSkills?: SkillInfo[], home = os.homedir(), options: SkillDiscoveryOptions = {}): DiscoveredSkill[] {
  return discoverSkillStates(cwd, piSkills, home, options).filter(skill => skill.enabled);
}

/** Re-read before approval so stale UI revisions cannot authorize changed instructions. */
export function reviewSkillSource(cwd: string, sourceId: string, revision: string, scope: 'project' | 'global', piSkills?: SkillInfo[], options: SkillDiscoveryOptions = {}): void {
  const candidate = discoverSkillCandidates(cwd, piSkills, options.homeDir ?? os.homedir(), options).find(skill => skill.sourceId === sourceId);
  if (!candidate || candidate.status === 'unavailable' || candidate.parseStatus !== 'valid' || !candidate.path) throw new Error('Skill source is unavailable or invalid');
  if (candidate.scope === 'workspace' && options.trusted === false) throw new Error('Trust the workspace before reviewing its skills');
  if (candidate.revision !== revision) throw new Error('Skill source changed; review its current definition');
  reviewCapabilitySource(options.db ?? openOctocodeDb(), scope === 'global' ? '*' : path.resolve(cwd), { kind: 'skill', sourceId, revision, name: candidate.name, path: candidate.path }, { enabled: true, select: true });
}
