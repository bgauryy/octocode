/** Stable agent-facing command and output guidance shared by CLI views. */
export const AGENT_TOOL_COMMANDS = {
  catalog: 'tools --json --compact',
  schema: 'tools <name> --scheme --json --compact',
  fullSchema: 'tools <name> --scheme --json',
  run: "tools <name> --queries '<json>' --compact",
  runEnvelope: "tools <name> --queries '<json>' --json",
} as const;

export const CONTINUATION_GUIDANCE =
  'Follow row data.next only while row data.pagination.hasMore. responsePagination windows content[].text only and remains visible in structuredContent.';

export const BATCH_ERROR_GUIDANCE =
  'Input validation rejects the call; runtime row errors stay indexed and isolated.';

export const CHEAP_VIEW_GUIDANCE =
  'Cheap views: compact schemas; localSearch resultView:"discovery"; content readers minify:"symbols".';
