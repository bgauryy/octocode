import path from 'node:path';
import { capabilityDefinitionRevision } from '../contracts/capability-sources.js';
import { listMcpOverrides } from '../contracts/mcp-state.js';
import type { PiContext, SkillInfo } from '../types.js';
import { refreshCapabilityAdapters } from '../adapters/pi-capability-adapters.js';
import { discoverSkillCandidates } from './skill-discovery.js';
import { configSignature, loadMcpConfig } from './mcp/config.js';
import { openOctocodeDb } from './storage-policy.js';

/** A review stamp covers definitions and effective preferences, never raw secrets. */
export async function configurationRevision(ctx?: PiContext, piSkills?: SkillInfo[]): Promise<string> {
  const cwd = path.resolve(ctx?.cwd ?? process.cwd());
  const loaded = await loadMcpConfig(ctx);
  const skills = discoverSkillCandidates(cwd, piSkills, undefined, { trusted: ctx?.isProjectTrusted?.() === true });
  const adapters = refreshCapabilityAdapters(ctx);
  let overrides: unknown;
  try { overrides = listMcpOverrides(openOctocodeDb(), cwd); } catch { overrides = 'unavailable'; }
  return capabilityDefinitionRevision({
    mcp: [...loaded.configuredServers].map(([name, config]) => [name, configSignature(config), loaded.servers.has(name)]),
    overrides,
    skills: skills.map(skill => [skill.sourceId, skill.revision, skill.status, skill.enabled, skill.selected]),
    hooks: adapters.hooks.snapshot().sources,
    models: adapters.models.snapshot().sources,
  });
}
