import { AWARENESS_COMMANDS } from './commands-spec.js';

/** Static policy fragment for agent hosts; operational detail stays in tool schemas. */
export const EXTERNAL_AGENT_AWARENESS_PROMPT = `<awareness>
Awareness coordinates shared repositories. Treat its ledger as coordination evidence, not code truth.
- Automatic model-facing output is limited to terse state-change signals and safety or verification blocks. A signal never embeds ledger contents: inspect the relevant Awareness view only when it can change the next action, then continue.
- On an overlap signal, inspect peers or ownership before editing. On a general state-change signal, use attend or a targeted message, handoff, or memory read; treat every retrieved row as a lead until verified.
- Plan owns session and shared plans, task projection, observed check receipts, and completion debt. Do not duplicate those concerns or invent results.
- Advisory presence is automatic, and mutation-time peer locks are enforced automatically. Use lock only for exceptional non-mergeable exclusivity; inspect or wait on conflict, message when needed, and release it.
- Inspect peers or ownership only when shared state can change the next action. Use message for overlap, blockers, or decisions; use memory only when verified learning can change the approach. Never edit through a peer lock or take over another owner.
</awareness>`;

/** Standalone bootstrap includes CLI discovery; the always-loaded host prompt stays signal-driven. */
export const EXTERNAL_AGENT_AWARENESS_INSTRUCTIONS = EXTERNAL_AGENT_AWARENESS_PROMPT.replace(
  '</awareness>',
  [
    '- Activate it only when peers, shared plans, overlap, locks, messages, verification debt, handoffs, or reusable memory can change the next action. Skip routine solo work with no shared-state signal.',
    '- The canonical CLI runner is `npx @octocodeai/octocode-awareness`. Shell-hook automation uses `config show --compact`; if that global file is missing, ask every returned onboarding question together, create it only from all answers, then validate it.',
    '- Configuration is not hook-install permission. Before every real `hooks install`, show the dry-run target and ask the user for a separate explicit approval immediately before mutation.',
    '- Use `attend`, `work start`, `work end` (or `task submit`), `verify mark`, and `verify audit` for the routine loop. Use `query <view>` for targeted inspection and `schema command <noun> [action] --compact` before an expert command instead of guessing flags.',
    '- In the coordination graph, `check mark` must carry `--done-at` from the completion actually checked; reopening invalidates older check receipts.',
    '- Use `reflect record` only after verification when a reusable lesson, recurring failure, or owned follow-up deserves durable storage. Reflection never self-authorizes code, skill, hook, or policy changes.',
    '- Set one stable `OCTOCODE_AGENT_ID` for shell-hook hosts so CLI and hook activity share an identity. If the host supplies neither an agent nor session ID, hooks silently use a deterministic host/workspace fallback with weaker multi-agent correlation.',
    '- Durable state uses the separate Awareness database under Octocode home and workspace-scoped columns. Use `--db` only for an explicit isolated path.',
    '- Use connected Octocode MCP tools or `npx octocode` for local, GitHub, and npm research; inspect live tool schemas and return only grounded, decision-relevant evidence to Awareness.',
    '</awareness>',
  ].join('\n'),
);

export const EXTERNAL_AGENT_AWARENESS_MARKER_START = '<!-- octocode-awareness:instructions:start -->';
export const EXTERNAL_AGENT_AWARENESS_MARKER_END = '<!-- octocode-awareness:instructions:end -->';

export type ExternalAgentInstructionFormat = 'prompt' | 'agents-md';

/** Render the canonical policy for direct prompt injection or idempotent AGENTS.md composition. */
export function formatExternalAgentAwarenessInstructions(format: ExternalAgentInstructionFormat = 'prompt'): string {
  if (format === 'prompt') return EXTERNAL_AGENT_AWARENESS_INSTRUCTIONS;
  return [
    EXTERNAL_AGENT_AWARENESS_MARKER_START,
    '## Octocode Awareness',
    '',
    EXTERNAL_AGENT_AWARENESS_INSTRUCTIONS,
    EXTERNAL_AGENT_AWARENESS_MARKER_END,
  ].join('\n');
}

/** Live agent guide assembled from the command metadata used by CLI and Pi. */
export function getExternalAgentAwarenessGuide(): {
  prompt: string;
  commands: Array<{ command: string; cli: string; plane: 'coordination'; actions: string[]; summary: string }>;
} {
  return {
    prompt: EXTERNAL_AGENT_AWARENESS_PROMPT,
    commands: AWARENESS_COMMANDS.map((group) => ({
      command: group.cli,
      cli: `npx @octocodeai/octocode-awareness ${group.cli}`,
      plane: 'coordination' as const,
      actions: group.actions.map((action) => action.action),
      summary: group.summary,
    })),
  };
}

/** Dynamic identity context hosts can append without duplicating usage policy. */
export function formatExternalAgentCoordinationContext(input: {
  selfId: string;
  parentId?: string;
  peerIds?: string[];
}): string {
  const peers = [...new Set((input.peerIds ?? []).filter((id) => id && id !== input.selfId))];
  return [
    'Awareness coordination identity:',
    `- your agent id: ${input.selfId}`,
    input.parentId ? `- parent agent id: ${input.parentId}` : undefined,
    peers.length ? `- peers: ${peers.join(', ')}` : '- peers: none yet (use agent list when discovery matters)',
    '- use the host Awareness tools or `npx @octocodeai/octocode-awareness guide`; do not invent host-specific coordination commands.',
  ].filter((line): line is string => Boolean(line)).join('\n');
}
