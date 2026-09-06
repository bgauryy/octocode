/**
 * dynamic-catalog — a terse system-prompt projection of the agent's self-created
 * dynamic tools (callTool) and skills (skill type:"call").
 *
 * Both registries are read from disk while the initial system prompt is assembled.
 * That prompt is frozen for provider caching; in-session create/update results stay
 * visible in the transcript and the next session receives the refreshed projection.
 *
 * Token discipline: emits `''` when both registries are empty (the common case),
 * truncates descriptions, and caps the number of entries so a large registry can never
 * bloat the prompt — the agent can always `action:"list"` for the full set.
 */

import { listTools } from './dynamic-tools.js';
import { listSkills } from './dynamic-skills.js';
import { truncatePlainToWidth } from './render-helpers.js';
import { escapePromptMetadata } from './prompt-safety.js';

const MAX_ENTRIES_PER_KIND = 30;
const MAX_DESCRIPTION_CHARS = 100;

interface CatalogEntry {
  name: string;
  description: string;
  /** Usage count — most-used entries win when the cap trims the list. */
  uses: number;
}

function truncate(text: string): string {
  const oneLine = (text || '').replace(/\s+/g, ' ').trim();
  // Cell-width aware (CJK/emoji count 2) — a code-unit slice under-counts them.
  return truncatePlainToWidth(oneLine, MAX_DESCRIPTION_CHARS);
}

function renderSection(label: string, entries: CatalogEntry[]): string[] {
  if (entries.length === 0) return [];
  // Name-first ordering keeps the injected block byte-stable between turns
  // (live uses counters would reorder it and churn the provider prompt cache).
  const sorted = [...entries].sort((a, b) => a.name.localeCompare(b.name));
  const shown = sorted.slice(0, MAX_ENTRIES_PER_KIND);
  const lines = [`${label}:`, ...shown.map((e) => `- ${escapePromptMetadata(e.name)}: ${escapePromptMetadata(truncate(e.description))}`)];
  if (sorted.length > shown.length) {
    lines.push(`- …and ${sorted.length - shown.length} more (call action:"list")`);
  }
  return lines;
}

/**
 * Build the `<dynamic_capabilities>` block, or `''` when there are no dynamic tools or
 * skills. Reads both registries without an in-memory cache; the caller owns the
 * session-level prompt freeze.
 */
export function getDynamicCapabilitiesAddendum(installedSkillNames: Iterable<string> = []): string {
  let toolEntries: CatalogEntry[] = [];
  let skillEntries: CatalogEntry[] = [];
  try {
    toolEntries = listTools().map((t) => ({ name: t.name, description: t.description, uses: t.stats?.calls ?? 0 }));
  } catch {
    // A missing/corrupt tools registry must never break prompt assembly.
  }
  try {
    skillEntries = listSkills().map((s) => ({ name: s.name, description: s.description, uses: s.stats?.uses ?? 0 }));
  } catch {
    // Same for skills.
  }
  const installed = new Set([...installedSkillNames].map((name) => name.trim().toLowerCase()).filter(Boolean));
  skillEntries = skillEntries.filter((entry) => !installed.has(entry.name.trim().toLowerCase()));
  if (toolEntries.length === 0 && skillEntries.length === 0) return '';

  return [
    '<dynamic_capabilities>',
    'Self-created reusable capabilities available this session (via callTool / skill type:"call"). ' +
      'Their list modes expose full schemas and steps.',
    ...renderSection('tools', toolEntries),
    ...renderSection('skills', skillEntries),
    '</dynamic_capabilities>',
  ].join('\n');
}
