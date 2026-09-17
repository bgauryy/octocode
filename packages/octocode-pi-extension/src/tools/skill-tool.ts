import { truncateToWidth } from '../tui/width.js';
import { paint } from '../tui/palette.js';
import fs from 'node:fs';
import path from 'node:path';
import type { ToolDefinition, ToolCallResult, PiTheme, PiContext, SkillInfo } from '../types.js';
import { DIRECT_TOOL_DESCRIPTIONS, type registerUniqueTool } from './octocode-tools.js';

import { makeComponentRenderer } from './render-helpers.js';
import { buildQueryEnvelopeSchema, executeQueryBatch } from './query-envelope.js';
import { orchestrate } from './call-skill.js';
import { discoverSkills, type DiscoveredSkill } from './skill-discovery.js';
import { readSkillPage } from './skill-pages.js';
import { capabilityDefinitionRevision } from '../contracts/capability-sources.js';
import { isWorkerCapabilityClient, getCurrentWorkerCapabilities, refreshCurrentWorkerCapabilities, assertCurrentWorkerNativeTool } from './worker-capabilities.js';

import { z } from 'zod';
type RegisterFn = typeof registerUniqueTool;

// Page long instructions with an executable continuation.
const SKILL_CONTENT_CAP = 8_000;

const SKILL_FILE_LIST_CAP = 30;
const SKILL_FILE_LIST_CHAR_CAP = 1_600;

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

type SkillPartialReason = 'content-limit' | 'file-limit' | 'file-depth' | 'file-filter' | 'file-read-error';

function skillContinuation(tool: 'localFetch' | 'astSearch', query: Record<string, unknown>, why: string) {
  return {
    tool: 'MCPTool' as const,
    query: { queries: [{ action: 'call' as const, tool, arguments: { queries: [query] } }] },
    why,
  };
}

function listSkillFiles(dir: string): { files: string[]; partialReasons: SkillPartialReason[] } {
  const files: string[] = [];
  let fileChars = 0;
  const partialReasons = new Set<SkillPartialReason>();
  const walk = (current: string, prefix: string, depth: number): void => {
    if (depth > 2) { partialReasons.add('file-depth'); return; }
    if (files.length >= SKILL_FILE_LIST_CAP) { partialReasons.add('file-limit'); return; }
    let entries: fs.Dirent[];
    try {
      entries = fs.readdirSync(current, { withFileTypes: true });
    } catch {
      partialReasons.add('file-read-error');
      return;
    }
    for (const entry of entries) {
      if (files.length >= SKILL_FILE_LIST_CAP) { partialReasons.add('file-limit'); return; }
      if (entry.name.startsWith('.')) { partialReasons.add('file-filter'); continue; }
      const rel = prefix ? `${prefix}/${entry.name}` : entry.name;
      if (entry.isDirectory()) walk(path.join(current, entry.name), rel, depth + 1);
      else if (entry.name !== 'SKILL.md') {
        if (fileChars + rel.length + 2 > SKILL_FILE_LIST_CHAR_CAP) { partialReasons.add('file-limit'); return; }
        files.push(rel);
        fileChars += rel.length + 2;
      }
    }
  };
  walk(dir, '', 0);
  return { files, partialReasons: [...partialReasons] };
}

function loadSkill(skill: DiscoveredSkill): ToolCallResult {
  let text: string;
  try {
    text = fs.readFileSync(skill.path, 'utf8');
    if (skill.revision && skill.revision !== capabilityDefinitionRevision({ raw: text, realPath: fs.realpathSync(skill.path) })) return result('Skill source changed during loading; refresh the catalog and review its current definition.', undefined, true);
  } catch (error) {
    return result(`skill "${skill.name}": cannot read ${skill.path}: ${(error as Error).message}`, undefined, true);
  }
  let returnedChars = Math.min(text.length, SKILL_CONTENT_CAP);
  if (returnedChars < text.length && (text.charCodeAt(returnedChars - 1) & 0xFC00) === 0xD800) returnedChars -= 1;
  const { files, partialReasons: filePartialReasons } = listSkillFiles(skill.dir);
  const buildPage = () => {
    const contentPartial = returnedChars < text.length;
    const partialReasons: SkillPartialReason[] = [...(contentPartial ? ['content-limit' as const] : []), ...filePartialReasons];
    const next = {
      ...(contentPartial ? { content: skillContinuation('localFetch', {
        path: skill.path, minify: 'none', chunkType: 'bytes', offset: Buffer.byteLength(text.slice(0, returnedChars)), limit: SKILL_CONTENT_CAP,
      }, 'Read the next page of skill instructions before acting.') } : {}),
      ...(filePartialReasons.length ? { files: skillContinuation('astSearch', {
        operation: 'files', path: skill.dir, entryType: 'f', excludeDir: [], maxDepth: 100, limit: 10_000, pageSize: 50, sort: 'path',
      }, 'Discover all supporting files; merge with this preview and follow returned continuations.') } : {}),
    };
    const isPartial = partialReasons.length > 0;
    const lines = [
      isPartial ? `next: ${JSON.stringify(next)}` : '',
      `skill: ${skill.name} [${skill.source}]`,
      `directory: ${skill.dir}`,
      `Resolve every relative path in the skill against this directory. ${contentPartial ? 'Read the remaining instructions before following the skill.' : 'Follow the skill now.'}`,
      files.length > 0 ? `files: ${files.join(', ')}` : '',
      isPartial ? `partial: ${partialReasons.join(', ')}; returned instruction characters: ${returnedChars}/${text.length}` : '',
      '---',
      text.slice(0, returnedChars),
    ].filter(Boolean);
    return result(lines.join('\n'), {
      name: skill.name, dir: skill.dir, files, isPartial, partialReasons,
      content: { returnedChars, totalChars: text.length, isPartial: contentPartial },
      fileDiscovery: { returnedFiles: files.length, isPartial: filePartialReasons.length > 0, partialReasons: filePartialReasons },
      ...(isPartial ? { next } : {}),
    });
  };
  const page = buildPage();
  recordSkillLoad(skill.name);
  return page;
}

function formatSkillList(skills: DiscoveredSkill[], total = skills.length): string {
  if (skills.length === 0) return 'No skills discovered. Install with: npx octocode skill install <skill> --platform pi';
  const lines = skills.map((skill) => {
    const used = usage.get(skill.name);
    const usedNote = used ? ` (loaded ${used.count}× this session)` : '';
    return `- ${skill.name} [${skill.source}] id:${skill.sourceId ?? skill.path}${usedNote}: ${skill.description || '(no description)'}`;
  });
  return [`${total} skill(s) available — load one with skill({queries:[{type:"load", action:"load", name:"…"}]}) when the task matches:`, ...lines].join('\n');
}

// ─── Per-query executors ───────────────────────────────────────────────────────

function executeLoadItem(
  query: Record<string, unknown>,
  cwd: string,
  getPiSkills: () => SkillInfo[] | undefined,
  ctx?: PiContext,
): ToolCallResult {
  const action = query['action'] === 'list' ? 'list' : 'load';
  const reason = typeof query['reason'] === 'string' ? query['reason'].trim() : '';
  if (action === 'load' && !reason) return result('skill load requires reason describing why the workflow matches.', undefined, true);
  const skills: DiscoveredSkill[] = isWorkerCapabilityClient()
    ? (getCurrentWorkerCapabilities()?.snapshot.skills ?? []).map(skill => ({ ...skill, sourceId: skill.id, description: skill.description ?? '', dir: path.dirname(skill.path), source: 'parent grant' }))
    : discoverSkills(cwd, getPiSkills(), undefined, { trusted: ctx?.isProjectTrusted?.() === true });
  if (action === 'list') {
    const page = readSkillPage(skills, query);
    const continuation = page.partial ? `\n${JSON.stringify({ partial: true, fragment: page.fragment, diagnostic: page.diagnostic, next: page.next })}` : '';
    return result((page.diagnostic?.message ?? formatSkillList(page.skills, page.total)) + continuation, page, Boolean(page.diagnostic));
  }
  const name = typeof query['name'] === 'string' ? query['name'].trim() : '';
  if (!name) return result('skill load requires name. Use skill({queries:[{type:"load", action:"list"}]}) for the catalog.', undefined, true);
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
  signal?: AbortSignal,
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

  const outcome = await orchestrate(params, ctx, signal);
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
  registeredToolNames: Set<string>,
  registerFn: RegisterFn,
  getPiSkills: () => SkillInfo[] | undefined,
): void {
  // ── Per-item schema: type:"load" | type:"call" with explicit typed fields ──
  const itemSchema = z.looseObject({
    type: z.enum(['load', 'call']).optional().describe('load: SKILL.md skills. call: dynamic skill lifecycle.'),
    action: z.enum(['load', 'list']).optional().describe('load: SKILL.md preview. list: catalog.'),
    offset: z.number().int().min(0).optional().describe('Continuation row.'),
    textOffset: z.number().int().min(0).optional().describe('Text offset.'),
    limit: z.number().int().min(1).max(50).optional().describe('Page size.'),
    catalogRevision: z.string().optional().describe('Catalog revision.'),
    name: z.string().optional().describe('Skill name.'),
    reason: z.string().optional().describe('Why the skill matches or why creation is needed.'),
    skillType: z.string().optional().describe('Workflow id for type:call.'),
    mode: z.enum(['auto', 'use', 'create', 'enhance', 'fix', 'list', 'delete']).optional().describe('auto·use·create·enhance·fix·list·delete'),
    intent: z.string().optional().describe('type:call workflow intent.'),
    approveCreate: z.boolean().optional().describe('Attest user approval for creation.'),
    force: z.boolean().optional().describe('Bypass triviality heuristic only.'),
  });

  const parameters = buildQueryEnvelopeSchema(itemSchema);

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
      preflight(query) {
        // Preserve single-query error results; multi-query calls must reject
        // malformed later items before any dynamic skill lifecycle effects.
        if ((params['queries'] as unknown[]).length < 2) return;
        itemSchema.parse(query);
        const type = query['type'] ?? 'load';
        const text = (key: string) => typeof query[key] === 'string' && (query[key] as string).trim().length > 0;
        if (type === 'call') {
          if (query['mode'] !== 'list' && !text('skillType')) throw new Error('skill call requires skillType.');
          if (['create', 'enhance', 'fix'].includes(String(query['mode'])) && !text('reason')) throw new Error('Skill creation requires reason.');
        } else if (query['action'] !== 'list') {
          if (!text('name')) throw new Error('skill load requires name.');
          if (!text('reason')) throw new Error('skill load requires reason describing why the workflow matches.');
        }
      },
      execute: async (query) => {
        if (isWorkerCapabilityClient()) {
          await refreshCurrentWorkerCapabilities({ signal });
          assertCurrentWorkerNativeTool('skill');
        }
        const type = typeof query['type'] === 'string' ? query['type'] : 'load';
        if (type === 'call') {
          if (isWorkerCapabilityClient()) return result('Dynamic skill authoring is parent-owned. Request the required skill from the parent.', undefined, true);
          return executeCallItem(query, ctx, signal);
        }
        const output = executeLoadItem(query, cwd, getPiSkills, ctx);
        if (isWorkerCapabilityClient() && output.isError) throw new Error(output.content.filter(part => part.type === 'text').map(part => part.text).join('\n'));
        return output;
      },
    });
  };

  const renderCall = (args: unknown, theme?: PiTheme) => {
    const queries = ((args ?? {}) as Record<string, unknown>)['queries'];
    const items = Array.isArray(queries) ? queries as Record<string, unknown>[] : [];
    const item = items[0];
    if (!item) {
      return makeComponentRenderer((_props, { width: width }) => [truncateToWidth(`${paint(theme, 'brand', '◆ skill')}`, width)], undefined);
    }
    const type = String(item['type'] ?? 'load');
    if (type === 'call') {
      const skillType = String(item['skillType'] ?? '?');
      const mode = typeof item['mode'] === 'string' ? ` ${item['mode']}` : '';
      return makeComponentRenderer((_props, { width: width }) => [truncateToWidth(
        `${paint(theme, 'brand', '◆ skill')} ${paint(theme, 'dim', '·')} ${paint(theme, 'title', `call:${skillType}`)}${paint(theme, 'dim', mode)}`, width)], undefined);
    }
    const target = item['action'] === 'list' ? 'list' : String(item['name'] ?? '?');
    return makeComponentRenderer((_props, { width: width }) => [truncateToWidth(
      `${paint(theme, 'brand', '◆ skill')} ${paint(theme, 'dim', '·')} ${paint(theme, 'title', target)}`, width)], undefined);
  };

  const renderResult = (resultValue: ToolCallResult, opts: { expanded?: boolean; isPartial?: boolean }, theme?: PiTheme) => {
    const text = (resultValue.content[0] as { text?: string } | undefined)?.text ?? '';
    const head = text.split('\n')[0] ?? 'skill';

    if (!resultValue.isError) {
      const status = String((resultValue.details as Record<string, unknown> | undefined)?.['status'] ?? '');
      const dynamicCallStatuses = new Set(['reuse', 'created', 'proposal', 'declined', 'listed', 'deleted']);
      if (!dynamicCallStatuses.has(status)) return makeComponentRenderer((_props, _context) => [], undefined);
      return makeComponentRenderer((_props, { width: width }) => [truncateToWidth(
        `${paint(theme, 'success', '✓')} ${paint(theme, 'title', 'skill')} ${paint(theme, 'dim', `· ${head}`)}`,
        width,
      )], undefined);
    }

    return makeComponentRenderer((_props, { width: width }) => {
      const lines = [truncateToWidth(`${paint(theme, 'error', '✗')} ${paint(theme, 'title', 'skill')} ${paint(theme, 'dim', `· ${head}`)}`, width)];
      if (opts.expanded) {
        for (const line of text.split('\n').slice(1, 12)) lines.push(truncateToWidth(paint(theme, 'dim', line), width));
      }
      return lines;
    }, undefined);
  };

  registerFn(pi, registeredToolNames, {
    name: 'skill',
    label: 'skill',
    description: DIRECT_TOOL_DESCRIPTIONS.skill!,
    promptSnippet: 'Load a specialized workflow when needed; manage recurring dynamic skills on request.',
    promptGuidelines: [
      'type:load selects installed SKILL.md by exact name. Read continuation pages before following a partial skill.',
      'type:call for recurring workflows; mode:create requires user approval.',
    ],
    parameters,
    execute,
    renderCall,
    renderResult,
  });
}
