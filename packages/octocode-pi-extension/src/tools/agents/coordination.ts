/** Append the handback destination without inventing communication recipient IDs. */
export function withWorkerCoordination(
  task: string,
  opts: { handbackPath?: string } = {},
): string {
  const lines = [
    'When bound communication tools are available, call peers to discover recipient session IDs. Local worker identities are not communication recipient IDs.',
    opts.handbackPath ? `- durable handback file: ${opts.handbackPath}` : undefined,
    opts.handbackPath
      ? '- before a terminal [DONE]/[BLOCKED]/[FAILED] when findings are long or important, write concise Markdown to that exact file (Status, Result, Evidence, Verification, Next), then include `[ARTIFACT] <path>` in your final output.'
      : undefined,
  ].filter((line): line is string => Boolean(line));
  return `${task}\n\n${lines.join('\n')}`;
}
