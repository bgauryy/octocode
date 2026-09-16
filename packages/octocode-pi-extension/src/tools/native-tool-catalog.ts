/**
 * native-tool-catalog — a system-prompt projection of all active native Pi tools.
 *
 * Mirrors the MCP catalog index (<mcp_catalog_index>) and available-skills
 * (<available_skills>) patterns: every active tool is listed with its name and
 * description so the model can route to the right tool without relying solely on
 * Pi's auto-generated Guidelines section.
 *
 * Rendered once per turn; the active tool set is stable across turns in normal
 * use (it only changes when capability grants change), so the output preserves
 * provider prompt-cache hits.
 */

import { DIRECT_TOOL_DESCRIPTIONS } from './octocode-tools.js';
import { escapePromptMetadata } from './prompt-safety.js';

/**
 * Build the `<native_tools>` block from the set of active tool names.
 * Returns `''` when no active tool has a registered description
 * (e.g. a worker that was granted zero native tools).
 *
 * Output is alphabetically sorted by name so bytes stay identical between
 * turns when the active set has not changed — this preserves provider
 * prompt-cache hits.
 */
export function renderNativeToolsAddendum(activeTools: Iterable<string>): string {
  const entries: Array<{ name: string; description: string }> = [];

  for (const name of activeTools) {
    const description = DIRECT_TOOL_DESCRIPTIONS[name];
    if (description) entries.push({ name, description });
  }

  if (entries.length === 0) return '';

  entries.sort((a, b) => a.name.localeCompare(b.name));

  return [
    '<native_tools>',
    'Active host tools available this session. Consult each tool\'s own schema for exact fields, options, and constraints.',
    ...entries.map(e => `- ${escapePromptMetadata(e.name)}: ${escapePromptMetadata(e.description)}`),
    '</native_tools>',
  ].join('\n');
}
