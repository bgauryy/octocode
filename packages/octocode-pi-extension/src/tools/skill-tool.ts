import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import {
  defaultAgentSkillSources,
  discoverAgentSkillInventory,
  type AgentSkillSourceDescriptor,
} from '@octocodeai/octocode-shared/agent-skills';
import type { ToolDefinition, ToolCallResult, PiTheme, PiContext, SkillInfo, TSchema } from '../types.js';
import { getAssetPaths } from '../assets.js';
import type { registerUniqueTool } from './octocode-tools.js';
import { stringEnumSchema } from './schema-helpers.js';
import { paint } from '../tui/cli-design.js';
import { makeRenderer, truncateToWidth } from './render-helpers.js';
import { isPromptOwnedSkill } from './skill-catalog.js';
import { buildQueryEnvelopeSchema, executeQueryBatch } from './query-envelope.js';
import { orchestrate } from './call-skill.js';
import { getSkillEnablement } from '@octocodeai/octocode-awareness/mcp-state';
import { openOctocodeDb } from './storage-policy.js';

type TypeBoxBuilder = (typeof import('typebox'))['Type'];
type RegisterFn = typeof registerUniqueTool;

export interface DiscoveredSkill {
  name: string;
  description: string;

  path: string;

  dir: string;

  source: string;
}

export interface DiscoveredSkillState extends DiscoveredSkill {
  enabled: boolean;
}

const SKILL_CONTENT_CAP = 48_000;

const SKILL_FILE_LIST_CAP = 30;

function skillKey(name: string): string {
  return name.replace(/\s+/g, ' ').trim().toLowerCase();
}

export function skillDiscoveryRoots(cwd: string, home = os.homedir()): Array<{ dir: string; source: string }> {
  return defaultAgentSkillSources(cwd, home).map((source) => ({
    dir: source.root,
    source: source.id,
  }));
}

export function discoverAllSkills(cwd: string, piSkills?: SkillInfo[], home = os.homedir()): DiscoveredSkill[] {
  const found = new Map<string, DiscoveredSkill>();
  const piMetadata = new Set<string>();
  const piConcrete = new Set<string>();
  for (const skill of piSkills ?? []) {
    const name = skill.name?.trim();
    if (!name || isPromptOwnedSkill(name)) continue;
    const md = (skill as { path?: string; filePath?: string }).path
      ?? (skill as { path?: string; filePath?: string }).filePath ?? '';
    const key = skillKey(name);
    piMetadata.add(key);
    if (md) piConcrete.add(key);
    found.set(key, {
      name,
      description: skill.description ?? '',
      path: md,
      dir: md ? path.dirname(md) : '',
      source: [skill.source, skill.scope].filter(Boolean).join('/') || 'pi',
    });
  }
  const sources: AgentSkillSourceDescriptor[] = defaultAgentSkillSources(cwd, home);
  for (const [root, scope] of [
    [path.join(home, '.octocode', 'skills'), 'user'],
    [path.join(cwd, '.octocode', 'skills'), 'workspace'],
  ] as const) {
    sources.push({
      id: `octocode:${scope}:${root}`,
      vendor: 'octocode',
      scope,
      root,
      precedence: sources.length,
      defaultEnabled: true,
    });
  }
  try {
    sources.push({
      id: 'pi:bundled',
      vendor: 'pi',
      scope: 'user',
      root: getAssetPaths().skillsDir,
      precedence: sources.length,
      defaultEnabled: true,
    });
  } catch (error) {
    // A source-less development build has no bundled skill directory.
    // Warn so the gap is not invisible during development.
    console.warn('[octocode:skills] bundled skill directory unavailable (dev build without dist/skills?):', (error as Error)?.message ?? error);
  }
  for (const skill of piSkills ?? []) {
    const file = (skill as { path?: string; filePath?: string }).path
      ?? (skill as { path?: string; filePath?: string }).filePath;
    if (!file || path.basename(file) !== 'SKILL.md') continue;
    const root = path.dirname(path.dirname(path.resolve(file)));
    if (sources.some((source) => path.resolve(source.root) === root)) continue;
    sources.push({
      id: `pi:runtime:${root}`,
      vendor: 'pi',
      scope: 'user',
      root,
      precedence: sources.length,
      defaultEnabled: true,
    });
  }
  const inventory = discoverAgentSkillInventory(sources, () => true);
  for (const entry of [...inventory.entries].sort((left, right) => left.precedence - right.precedence)) {
    if (!entry.enabled || entry.parseStatus !== 'valid' || !entry.skill || isPromptOwnedSkill(entry.skill.name)) continue;
    const key = skillKey(entry.skill.name);
    const existing = found.get(key);
    if (piConcrete.has(key)) continue;
    const source = entry.source === 'pi:bundled'
      ? 'bundled'
      : entry.scope === 'workspace'
        ? entry.vendor === 'agents' ? 'project' : `project:${entry.vendor}`
        : entry.vendor === 'pi' ? 'user' : `user:${entry.vendor}`;
    found.set(key, existing && piMetadata.has(key)
      ? { ...existing, path: entry.skill.path, dir: entry.skill.dir }
      : {
          name: entry.skill.name,
          description: entry.skill.description,
          path: entry.skill.path,
          dir: entry.skill.dir,
          source,
        });
  }
  return [...found.values()].sort((a, b) => a.name.localeCompare(b.name));
}

export function discoverSkillStates(cwd: string, piSkills?: SkillInfo[], home = os.homedir()): DiscoveredSkillState[] {
  const skills = discoverAllSkills(cwd, piSkills, home);
  try {
    const db = openOctocodeDb();
    const scopeKey = path.resolve(cwd);
    return skills.map((skill) => ({ ...skill, enabled: getSkillEnablement(db, scopeKey, skill.name, true) }));
  } catch {
    return skills.map((skill) => ({ ...skill, enabled: true }));
  }
}

/** Effective loadable inventory. Disabled skills remain discoverable only in settings. */
export function discoverSkills(cwd: string, piSkills?: SkillInfo[], home = os.homedir()): DiscoveredSkill[] {
  return discoverSkillStates(cwd, piSkills, home).filter((skill) => skill.enabled);
}

export interface SkillUsageEntry {
  count: number;
  lastLoadedAt: number;
}

const usage = new Map<string, SkillUsageEntry>();

export function recordSkillLoad(name: string, now = Date.now()): void {
  const entry = usage.get(name) ?? { count: 0, lastLoadedAt: 0 };
  entry.count += 1;
  entry.lastLoadedAt = now;
  usage.set(name, entry);
}

export function getSkillUsage(): ReadonlyMap<string, SkillUsageEntry> {
  return usage;
}

export function resetSkillUsageForTests(): void {
  usage.clear();
}

export function formatSkillUsageLines(): string[] {
  return [...usage.entries()]
    .sort((a, b) => b[1].lastLoadedAt - a[1].lastLoadedAt)
    .slice(0, 10)
    .map(([name, entry]) => `- ${name}: loaded ${entry.count}×`);
}

function result(text: string, details?: unknown, isError = false): ToolCallResult {
  return { content: [{ type: 'text', text }], details, isError };
}

function listSkillFiles(dir: string): string[] {
  const files: string[] = [];
  const walk = (current: string, prefix: string, depth: number): void => {
    if (depth > 2 || files.length >= SKILL_FILE_LIST_CAP) return;
    let entries: fs.Dirent[];
    try {
      entries = fs.readdirSync(current, { withFileTypes: true });
    } catch {
      return;
    }
    for (const entry of entries) {
      if (files.length >= SKILL_FILE_LIST_CAP) return;
      if (entry.name.startsWith('.')) continue;
      const rel = prefix ? `${prefix}/${entry.name}` : entry.name;
      if (entry.isDirectory()) walk(path.join(current, entry.name), rel, depth + 1);
      else if (entry.name !== 'SKILL.md') files.push(rel);
    }
  };
  walk(dir, '', 0);
  return files;
}

function loadSkill(skill: DiscoveredSkill): ToolCallResult {
  let text: string;
  try {
    text = fs.readFileSync(skill.path, 'utf8');
  } catch (error) {
    return result(`skill "${skill.name}": cannot read ${skill.path}: ${(error as Error).message}`, undefined, true);
  }
  const capped = text.length <= SKILL_CONTENT_CAP
    ? text
    : `${text.slice(0, SKILL_CONTENT_CAP)}\n…[truncated ${text.length - SKILL_CONTENT_CAP} chars — read the rest from ${skill.path} if needed]`;
  const files = listSkillFiles(skill.dir);
  recordSkillLoad(skill.name);
  const lines = [
    `skill: ${skill.name} [${skill.source}]`,
    `directory: ${skill.dir}`,
    'Resolve every relative path in the skill against this directory. Follow the skill now.',
    files.length > 0 ? `files: ${files.join(', ')}` : '',
    '---',
    capped,
  ].filter(Boolean);
  return result(lines.join('\n'), { name: skill.name, dir: skill.dir, files });
}

function formatSkillList(skills: DiscoveredSkill[]): string {
  if (skills.length === 0) return 'No skills discovered. Install with: npx octocode skill install <skill> --platform pi';
  const lines = skills.map((skill) => {
    const used = usage.get(skill.name);
    const usedNote = used ? ` (loaded ${used.count}× this session)` : '';
    return `- ${skill.name} [${skill.source}]${usedNote}: ${skill.description || '(no description)'}`;
  });
  return [`${skills.length} skill(s) available — load one with skill({queries:[{reasoning:"load matching skill", type:"load", action:"load", name:"…", reason:"why it matches"}]}) when the task matches:`, ...lines].join('\n');
}

// ─── Per-query executors ───────────────────────────────────────────────────────

function executeLoadItem(
  query: Record<string, unknown>,
  cwd: string,
  getPiSkills: () => SkillInfo[] | undefined,
): ToolCallResult {
  const action = query['action'] === 'list' ? 'list' : 'load';
  const skills = discoverSkills(cwd, getPiSkills());
  if (action === 'list') return result(formatSkillList(skills), { skills });
  const name = typeof query['name'] === 'string' ? query['name'].trim() : '';
  if (!name) return result('skill load requires name. Use skill({queries:[{reasoning:"…", type:"load", action:"list"}]}) for the catalog.', undefined, true);
  const reason = typeof query['reason'] === 'string' ? query['reason'].trim() : '';
  if (!reason) return result('skill load requires reason explaining why it matches the current task.', undefined, true);
  const skill = skills.find((candidate) => candidate.name === name)
    ?? skills.find((candidate) => candidate.name.toLowerCase() === name.toLowerCase());
  if (!skill) {
    return result(`Unknown skill: ${name}\nAvailable: ${skills.map((s) => s.name).join(', ') || 'none'}`, { skills: skills.map((s) => s.name) }, true);
  }
  if (!skill.path) return result(`skill "${skill.name}" has no resolvable SKILL.md path.`, undefined, true);
  return loadSkill(skill);
}

async function executeCallItem(
  query: Record<string, unknown>,
  ctx?: PiContext,
): Promise<ToolCallResult> {
  const skillType = typeof query['skillType'] === 'string' ? query['skillType'].trim() : '';
  const mode = typeof query['mode'] === 'string' ? query['mode'] : undefined;
  // Explicit typed fields (replaces nested metadata)
  const intent = typeof query['intent'] === 'string' ? query['intent'] : '';
  const reason = typeof query['reason'] === 'string' ? query['reason'] : '';
  const approveCreate = query['approveCreate'] === true;
  const force = query['force'] === true;

  const params = {
    skillType,
    mode: mode as 'auto' | 'use' | 'create' | 'enhance' | 'fix' | 'list' | 'delete' | undefined,
    metadata: {
      intent,
      reason,
      _approveCreate: approveCreate,
      _force: force,
    },
  };

  const outcome = await orchestrate(params, ctx);
  const parts: string[] = [renderCallOutcomeHeader(outcome as unknown as Record<string, unknown>)];
  if (outcome.status === 'listed') {
    parts.push(
      (outcome.skills ?? []).map(
        (s) => `  ${s.name} v${s.version} — ${s.description} (uses ${s.uses})`,
      ).join('\n') || '  (no dynamic skills)',
    );
  }
  if (outcome.pruned && outcome.pruned.length > 0) {
    parts.push(`[MAINTAINED] pruned broken skills: ${outcome.pruned.join(', ')}`);
  }
  return {
    content: [{ type: 'text', text: parts.join('\n') }],
    isError: outcome.status === 'error',
    details: outcome,
  } as unknown as ToolCallResult;
}

function renderCallOutcomeHeader(o: Record<string, unknown>): string {
  const status = String(o['status'] ?? '');
  const message = String(o['message'] ?? '');
  switch (status) {
    case 'reuse':    return `[REUSE] ${message}`;
    case 'created':  return `[CREATED] ${message}`;
    case 'proposal': return `[PROPOSAL] ${message}`;
    case 'declined': return `[DECLINED] ${message}`;
    case 'listed':   return `[SKILLS] ${((o['skills'] as unknown[]) ?? []).length} dynamic skill(s)`;
    case 'deleted':  return `[DELETED] ${message}`;
    default:         return `[ERROR] ${message}`;
  }
}

// ─── Tool registration ─────────────────────────────────────────────────────────

export function registerSkillTool(
  pi: { registerTool?(def: ToolDefinition): void },
  Type: TypeBoxBuilder,
  registeredToolNames: Set<string>,
  registerFn: RegisterFn,
  getPiSkills: () => SkillInfo[] | undefined,
): void {
  // ── Per-item schema: type:"load" | type:"call" with explicit typed fields ──
  const itemSchema = Type.Object({
    reasoning: Type.String({ minLength: 1, maxLength: 240, description: 'Concise reason this query is necessary.' }),
    type: Type.Optional(stringEnumSchema(
      Type,
      ['load', 'call'],
      'load (default): work with installed SKILL.md skills (load or list). call: manage dynamic skills (reuse, create, enhance, fix, list, delete).',
    ) as TSchema),
    // ── type:load fields ──
    action: Type.Optional(stringEnumSchema(
      Type,
      ['load', 'list'],
      'load (default): return one skill\'s full SKILL.md + directory + files. list: catalog of every discovered skill.',
    ) as TSchema),
    name: Type.Optional(Type.String({ description: 'Skill name for type:load action:load (exact name from <available_skills> or action:list).' })),
    reason: Type.Optional(Type.String({
      minLength: 1,
      description: 'Required for type:load action:load. One concise, user-facing clause explaining why this skill matches the current task. Also used as skill creation reason for type:call mode:create.',
    })),
    // ── type:call fields ──
    skillType: Type.Optional(Type.String({ description: 'Skill name / workflow id (lowercase a-z, 0-9, hyphens). Required for type:call.' })),
    mode: Type.Optional(stringEnumSchema(
      Type,
      ['auto', 'use', 'create', 'enhance', 'fix', 'list', 'delete'],
      'auto (default) · use (reuse only) · create (after user approval) · enhance/fix (revise existing) · list · delete.',
    ) as TSchema),
    intent: Type.Optional(Type.String({ description: 'What the workflow does (type:call). Guides skill-smith authoring and keyword matching.' })),
    approveCreate: Type.Optional(Type.Boolean({ description: 'Approve creation in auto mode without an extra roundtrip (type:call).' })),
    force: Type.Optional(Type.Boolean({ description: 'Override the triviality decline gate (type:call).' })),
  }, { additionalProperties: false }) as TSchema;

  const parameters = buildQueryEnvelopeSchema(Type, itemSchema, {
    maxItems: 100,
    reasoningDescription: 'Concise reason this query is necessary.',
  });

  const execute = async (
    toolCallId: string,
    params: Record<string, unknown>,
    signal?: AbortSignal,
    onUpdate?: unknown,
    ctx?: PiContext,
  ): Promise<ToolCallResult> => {
    const cwd = ctx?.cwd ?? process.cwd();
    return executeQueryBatch({
      raw: params,
      toolCallId,
      signal,
      onUpdate: onUpdate as ((r: ToolCallResult) => void) | undefined,
      ctx,
      passthroughSingle: true,
      execute: async (query) => {
        const type = typeof query['type'] === 'string' ? query['type'] : 'load';
        if (type === 'call') {
          return executeCallItem(query, ctx);
        }
        return executeLoadItem(query, cwd, getPiSkills);
      },
    });
  };

  const renderCall = (args: unknown, theme?: PiTheme) => {
    const queries = ((args ?? {}) as Record<string, unknown>)['queries'];
    const items = Array.isArray(queries) ? queries as Record<string, unknown>[] : [];
    const item = items[0];
    if (!item) {
      return makeRenderer((width) => [truncateToWidth(`${paint(theme, 'brand', '◆ skill')}`, width)]);
    }
    const type = String(item['type'] ?? 'load');
    if (type === 'call') {
      const skillType = String(item['skillType'] ?? '?');
      const mode = typeof item['mode'] === 'string' ? ` ${item['mode']}` : '';
      return makeRenderer((width) => [truncateToWidth(
        `${paint(theme, 'brand', '◆ skill')} ${paint(theme, 'dim', '·')} ${paint(theme, 'title', `call:${skillType}`)}${paint(theme, 'dim', mode)}`, width)]);
    }
    const target = item['action'] === 'list' ? 'list' : String(item['name'] ?? '?');
    return makeRenderer((width) => [truncateToWidth(
      `${paint(theme, 'brand', '◆ skill')} ${paint(theme, 'dim', '·')} ${paint(theme, 'title', target)}`, width)]);
  };

  const renderResult = (resultValue: ToolCallResult, opts: { expanded?: boolean; isPartial?: boolean }, theme?: PiTheme) => {
    const text = (resultValue.content[0] as { text?: string } | undefined)?.text ?? '';
    const head = text.split('\n')[0] ?? 'skill';

    if (!resultValue.isError) {
      const status = String((resultValue.details as Record<string, unknown> | undefined)?.['status'] ?? '');
      const dynamicCallStatuses = new Set(['reuse', 'created', 'proposal', 'declined', 'listed', 'deleted']);
      if (!dynamicCallStatuses.has(status)) return makeRenderer(() => []);
      return makeRenderer((width) => [truncateToWidth(
        `${paint(theme, 'success', '✓')} ${paint(theme, 'title', 'skill')} ${paint(theme, 'dim', `· ${head}`)}`,
        width,
      )]);
    }

    return makeRenderer((width) => {
      const lines = [truncateToWidth(`${paint(theme, 'error', '✗')} ${paint(theme, 'title', 'skill')} ${paint(theme, 'dim', `· ${head}`)}`, width)];
      if (opts.expanded) {
        for (const line of text.split('\n').slice(1, 12)) lines.push(truncateToWidth(paint(theme, 'dim', line), width));
      }
      return lines;
    });
  };

  registerFn(pi, registeredToolNames, {
    name: 'skill',
    label: 'skill',
    description: [
      'Unified skill facade: load installed Agent Skills or manage dynamic workflow skills in a single ordered batch.',
      '',
      'type:"load" (default) — Load an installed skill by name and explain why it matches the current task (returns its full SKILL.md, directory, and shipped files), or list every discovered skill. This is THE way to load a skill — do not hunt for SKILL.md paths manually.',
      '',
      'type:"call" — Meta-tool for reusable multi-step workflows: resolves an existing dynamic skill in O(1); on a miss it PROPOSES creation (never silently authors). After you research/brainstorm and the user confirms, re-call with mode:"create" and reason; a skill-smith authors the SKILL.md, which is registered ONLY if it passes frontmatter+structure validation. Every call prunes junk skills. Replaces explicit typed fields for intent, reason, approveCreate, and force (no more opaque metadata).',
    ].join('\n'),
    promptSnippet: [
      'skill is the unified skill facade with queries[] (each requiring reasoning):',
      '  type:"load" — load/list installed SKILL.md skills: skill({queries:[{reasoning:"…", type:"load", name:"…", reason:"…"}]})',
      '  type:"call" — manage dynamic skills: skill({queries:[{reasoning:"…", type:"call", skillType:"…", mode:"auto"}]})',
      'Load the minimal matching skill BEFORE acting; use type:"call" for recurring multi-step workflows not covered by an installed skill.',
    ].join('\n'),
    promptGuidelines: [
      'When loading a skill (type:"load"), pass reason as one concise, user-facing clause that explains why the skill matches the current task.',
      'Use type:"call" for recurring multi-step workflows; never for a single action a tool/bash/callTool already covers.',
      'On a creation proposal (type:"call"): research existing skills/tools/commands and brainstorm the smallest workflow, then ASK the user before re-calling with mode:"create" and a clear reason.',
      'Multi-query: run load and call operations in a single skill({queries:[…]}) call when they are logically related.',
    ],
    parameters,
    execute,
    renderCall,
    renderResult,
  });
}
