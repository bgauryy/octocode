import { formatExternalAgentCoordinationContext } from '@octocodeai/octocode-awareness/host';

/** Append bounded peer identities and the assigned durable handback path. */
export function withPeerCoordination(
  task: string,
  selfId: string | undefined,
  peerIds: string[],
  opts: { parentId?: string; handbackPath?: string } = {},
): string {
  if (!selfId) return task;
  const coordination = formatExternalAgentCoordinationContext({ selfId, parentId: opts.parentId, peerIds });
  const lines = [
    coordination,
    opts.handbackPath ? `- durable handback file: ${opts.handbackPath}` : undefined,
    opts.handbackPath
      ? '- before a terminal [DONE]/[BLOCKED]/[FAILED] when findings are long or important, write concise Markdown to that exact file (Status, Result, Evidence, Verification, Next), then include `[ARTIFACT] <path>` in your final output.'
      : undefined,
  ].filter((line): line is string => Boolean(line));
  return `${task}\n\n${lines.join('\n')}`;
}
