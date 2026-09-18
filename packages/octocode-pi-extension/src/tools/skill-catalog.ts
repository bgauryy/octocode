import { truncatePlainToWidth } from './render-helpers.js';
import { escapePromptMetadata } from './prompt-safety.js';

export interface SkillCatalogEntry {
  name: string;
  description?: string;
  source?: string;
  scope?: string;
}

// Dashboard caps: on-demand output, so it can afford the full picture.
const MAX_SKILLS = 80;
const MAX_DESCRIPTION_CHARS = 512;
// Prompt discovery refreshes each turn; oversized metadata is recoverable via list.
const MAX_PROMPT_DESCRIPTION_CHARS = 320;

function clean(text: string): string {
  return text.replace(/\s+/g, ' ').trim();
}

function truncate(text: string, limit = MAX_DESCRIPTION_CHARS): string {
  // Cell-width aware (CJK/emoji count 2) — a code-unit slice under-counts them.
  return truncatePlainToWidth(clean(text), limit);
}

function skillSortKey(skill: SkillCatalogEntry): string {
  return clean(skill.name).toLowerCase();
}

/**
 * Initial projection of effective loadable skills into the Octocode prompt layer.
 *
 * Pi already includes skills in its own prompt, but Octocode replaces/augments the
 * system prompt and also needs a compaction-durable reminder that every loaded
 * skill has a name + description and should be loaded when context matches. The
 * catalog is rebuilt from effective source revisions on each turn. In-session
 * loads remain in tool results.
 */
export function canonicalizeSkillCatalog(skills: SkillCatalogEntry[] | undefined): SkillCatalogEntry[] {
  const byName = new Map<string, SkillCatalogEntry>();
  for (const skill of skills ?? []) {
    const key = clean(skill.name).toLowerCase();
    if (!key || byName.has(key)) continue;
    byName.set(key, skill);
  }
  return [...byName.values()].sort((a, b) => skillSortKey(a).localeCompare(skillSortKey(b)));
}

function formatSkillLine(skill: SkillCatalogEntry, descriptionLimit = MAX_DESCRIPTION_CHARS): string {
  const promptDescription = skill.description;
  const description = promptDescription ? escapePromptMetadata(truncate(promptDescription, descriptionLimit)) : '(no description)';
  const source = [skill.source, skill.scope].filter(Boolean).join('/');
  return `- ${escapePromptMetadata(skill.name)}: ${description}${source ? ` [${escapePromptMetadata(source)}]` : ''}`;
}

export interface SkillsDashboardExtras {
  /** Session load-observability lines (e.g. "- octocode-research: loaded 2×"). */
  usageLines?: string[];
  /** Path of the machine-readable discovery inventory, when written. */
  discoveryPath?: string;
}

export function renderSkillsDashboard(skills: SkillCatalogEntry[] | undefined, extras: SkillsDashboardExtras = {}): string {
  const valid = canonicalizeSkillCatalog(skills);
  const shown = valid.slice(0, MAX_SKILLS);
  const usageLines = extras.usageLines ?? [];
  return [
    '◆ Octocode skills',
    '',
    'Available now',
    ...(shown.length > 0 ? shown.map((skill) => formatSkillLine(skill)) : ['(none discovered — run /reload after installing skills)']),
    ...(valid.length > shown.length ? [`- …and ${valid.length - shown.length} more skill(s)`] : []),
    '',
    'Loaded this session',
    ...(usageLines.length > 0 ? usageLines : ['(none yet — the agent loads them via the skill tool when a task matches)']),
    '',
    'How to use',
    'The agent loads enabled skills with skill({queries:[{type:"load", action:"load", name:"…", reason:"why it matches"}]}). Manage enablement in /configuration.',
    'Invoke a specific enabled skill with /skill:<name>.',
    'Install bundled skills with: npx octocode skill install <skill> --platform pi',
    'Discovery refreshes on the next turn; /reload also reloads Pi resources.',
    ...(extras.discoveryPath ? [`Machine-readable inventory (skills + MCP config + tools): ${extras.discoveryPath}`] : []),
  ].join('\n');
}

export function renderAvailableSkillsAddendum(skills: SkillCatalogEntry[] | undefined): string {
  const valid = canonicalizeSkillCatalog(skills);
  if (valid.length === 0) return '';

  const lines: string[] = [];
  let chars = 0;
  for (const skill of valid) {
    const line = formatSkillLine(skill, MAX_PROMPT_DESCRIPTION_CHARS);
    if (chars + line.length > 18_000) break;
    lines.push(line);
    chars += line.length + 1;
  }
  const partial = lines.length < valid.length || valid.some(skill => clean(skill.description ?? '').length > MAX_PROMPT_DESCRIPTION_CHARS);

  return [
    '<available_skills>',
    'Optional skills available by name. The skill tool can list the catalog or load a selected skill.',
    ...lines,
    ...(partial ? [`catalog_continuation: ${JSON.stringify({ partial: true, next: { tool: 'skill', params: { queries: [{ type: 'load', action: 'list' }] } } })}`] : []),
    '</available_skills>',
  ].join('\n');
}
