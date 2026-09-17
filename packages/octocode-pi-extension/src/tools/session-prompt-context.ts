import { assembleContextSegments, INITIAL_CONTEXT_TOKEN_BUDGET } from './context-segments.js';

const SEGMENTS = [
  // System policy — frozen, cache-stable. Hard cap matches typical rendered size.
  { id: 'octocode-product-policy', kind: 'product-policy', origin: 'octocode-harness', authority: 'product', visibility: 'hidden-policy', rehydrate: 'always', tokenBudget: 12_000 },
  // MCP routing index — renderMcpRoutingIndex caps output at 18K chars (~4.5K tokens); 6K gives headroom.
  { id: 'mcp-tool-contracts', kind: 'tool-contract', origin: 'octocode-harness', authority: 'product', visibility: 'inspectable', rehydrate: 'always', tokenBudget: 6_000 },
  // Runtime image/capability flags — 5 short lines.
  { id: 'runtime-tool-contracts', kind: 'tool-contract', origin: 'octocode-harness', authority: 'product', visibility: 'inspectable', rehydrate: 'always', tokenBudget: 500 },
  // Dynamic callTool/skill:call catalog — MAX_ENTRIES_PER_KIND=30 × MAX_DESCRIPTION_CHARS=100 each; usually empty.
  { id: 'dynamic-tool-contracts', kind: 'tool-contract', origin: 'octocode-harness', authority: 'product', visibility: 'inspectable', rehydrate: 'always', tokenBudget: 6_000 },
  // Installed skill list — renderAvailableSkillsAddendum caps at 18K chars (~4.5K tokens); 5K gives headroom.
  { id: 'available-skills', kind: 'skill', origin: 'installed-skills', authority: 'project', visibility: 'inspectable', rehydrate: 'on-trigger', tokenBudget: 5_000 },
  // Session memory/audit file paths — two short lines.
  { id: 'session-artifact-contract', kind: 'tool-contract', origin: 'octocode-harness', authority: 'product', visibility: 'inspectable', rehydrate: 'always', tokenBudget: 500 },
  // Awareness CLI runner binding — 5 short lines when bash-only path; inline disabled notice otherwise.
  { id: 'awareness-cli-runtime', kind: 'tool-contract', origin: 'octocode-harness', authority: 'product', visibility: 'inspectable', rehydrate: 'always', tokenBudget: 1_000 },
] as const;

export type SessionPromptContents = Record<(typeof SEGMENTS)[number]['id'], string> & { 'agents-protocol'?: string };

/** One policy/budget contract for initial prompts and recovery source validation. */
export function assembleSessionPromptContext(contents: SessionPromptContents) {
  return {
    ...assembleContextSegments([
      ...SEGMENTS.map(segment => ({ ...segment, scope: 'session' as const, content: contents[segment.id] })),
      { id: 'agents-protocol', kind: 'project-instruction', origin: 'agents-protocol', authority: 'user', visibility: 'inspectable', scope: 'session', rehydrate: 'always', tokenBudget: 12_000, content: contents['agents-protocol'] ?? '' },
    ], { totalTokenBudget: INITIAL_CONTEXT_TOKEN_BUDGET }),
    contents: { ...contents, 'agents-protocol': contents['agents-protocol'] ?? '' },
  };
}
