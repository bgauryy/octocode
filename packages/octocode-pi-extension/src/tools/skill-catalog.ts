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
const MAX_DESCRIPTION_CHARS = 180;
// The prompt is frozen after the complete initial discovery pass. Keep every
// skill name so routing is correct; descriptions alone are tightly bounded.
const MAX_PROMPT_DESCRIPTION_CHARS = 120;

const PROMPT_OWNED_SKILLS = new Set([
  // Pi owns Awareness through its prompt and tools; a user-installed copy must
  // not create a second coordination surface.
  'octocode-awareness',
]);

const PROMPT_DESCRIPTION_OVERRIDES: Readonly<Record<string, string>> = Object.freeze({
  // Pi discovery may prefer an older global copy over the package resource.
  // Keep the extension's model-facing routing manifest stable at this boundary.
  'octocode-architect': 'Use when root-cause analysis, boundary design, blast-radius mapping, architecture review, or interface contracts matter.',
  'octocode-brainstorming': 'Use when an idea needs options, feasibility testing, adjacent opportunities, or scope exploration before building.',
  'octocode-chrome-devtools': 'Use when live-page Chrome DevTools/CDP evidence is needed: console, network, DOM/CSS, performance, or automation.',
  'octocode-code-graph': 'Use when mapping dependencies, change impact, cycles, layering, dead code, reachability, or architecture risk.',
  'octocode-documentation': 'Use when creating, repairing, or reviewing READMEs, API docs, guides, comments, ADRs, runbooks, or stale technical docs.',
  'octocode-eval-benchmark': 'Use when measuring whether a change helped: define KPIs, baselines, held-out cases, benchmarks, and keep/discard gates.',
  'octocode-orchestrator': 'Use when substantial work needs one agent to coordinate workstreams, subagents, TDD, evaluation, and handoffs.',
  'octocode-prompt-optimizer': 'Use when improving prompts, tool/MCP schemas, policies, handoffs, routing descriptions, or instruction clarity.',
  'octocode-research': 'Use when a code claim must be proven: trace callers, imports, runtime wiring, regressions, GitHub, or change impact.',
  'octocode-rfc-generator': 'Use when consequential architecture, migration, public-contract, or multi-phase changes need a reviewed decision.',
  'octocode-roast': 'Use when a blunt evidence-backed code roast is wanted: rank smells, debt, hot paths, top sins, and cleanup priorities.',
  'octocode-scraping': 'Use when scraping public URLs/docs into a cited corpus, extracting tables/pricing, or diagnosing blocked/thin pages.',
  'octocode-skills': 'Use when Agent Skills/SKILL.md need finding, comparison, review, creation, repair, install, sync, or trigger tuning.',
  'octocode-subagent': 'Use when choosing execution before spawning agents: solo/batch/subagent/local-Ollama, decomposition, and handoffs.',
});

export function isPromptOwnedSkill(name: string): boolean {
  return PROMPT_OWNED_SKILLS.has(clean(name).toLowerCase());
}

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
 * complete catalog is frozen with the first system prompt; installs/removals
 * take effect in a new session, while in-session loads remain in tool results.
 */
export function canonicalizeSkillCatalog(skills: SkillCatalogEntry[] | undefined): SkillCatalogEntry[] {
  const byName = new Map<string, SkillCatalogEntry>();
  for (const skill of skills ?? []) {
    const key = clean(skill.name).toLowerCase();
    if (!key || isPromptOwnedSkill(skill.name) || byName.has(key)) continue;
    byName.set(key, skill);
  }
  return [...byName.values()].sort((a, b) => skillSortKey(a).localeCompare(skillSortKey(b)));
}

function formatSkillLine(skill: SkillCatalogEntry, descriptionLimit = MAX_DESCRIPTION_CHARS): string {
  const promptDescription = PROMPT_DESCRIPTION_OVERRIDES[clean(skill.name).toLowerCase()] ?? skill.description;
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
    'The agent loads enabled skills with skill({queries:[{reasoning:"load matching skill", type:"load", action:"load", name:"…", reason:"why it matches"}]}). Manage enablement in /mcp.',
    'Invoke a specific enabled skill with /skill:<name>.',
    'Install bundled skills with: npx octocode skill install <skill> --platform pi',
    'Refresh discovery with /reload after installs/removals.',
    ...(extras.discoveryPath ? [`Machine-readable inventory (skills + MCP config + tools): ${extras.discoveryPath}`] : []),
  ].join('\n');
}

export function renderAvailableSkillsAddendum(skills: SkillCatalogEntry[] | undefined): string {
  const valid = canonicalizeSkillCatalog(skills);
  if (valid.length === 0) return '';

  const lines = valid.map((skill) => formatSkillLine(skill, MAX_PROMPT_DESCRIPTION_CHARS));

  return [
    '<available_skills>',
    'Skills available by name this turn. Names/descriptions are enough to decide whether a skill matches; do not preload every skill body. Use this catalog with the <skills> policy: when the user names a skill or the task context matches a description, load the minimal matching skill BEFORE acting via skill({queries:[{reasoning:"load matching skill", type:"load", action:"load", name:"…", reason:"why it matches"}]}) — it returns bounded instructions, the skill directory, and files. Follow executable next calls when partial before acting. skill({queries:[{reasoning:"refresh skill catalog", type:"load", action:"list"}]}) refreshes the catalog with usage. Do not load skills as ceremony.',
    ...lines,
    '</available_skills>',
  ].join('\n');
}
