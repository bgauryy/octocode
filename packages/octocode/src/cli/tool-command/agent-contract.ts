/** Stable agent-facing command and output guidance shared by CLI views. */
export const AGENT_TOOL_COMMANDS = {
  catalog: 'tools --json --compact',
  schema: 'tools <name> --scheme --json --compact',
  fullSchema: 'tools <name> --scheme --json',
  run: "tools <name> --queries '<json>' --compact",
  runJson: "tools <name> --queries '<json>' --json",
} as const;

export const CONTINUATION_GUIDANCE =
  'Follow executable next.* continuations in row data and nested payloads when their pagination or partial state indicates more; scan/depth limits can require continuation even when pagination.hasMore is false. responsePagination only windows human-readable text; structured results remain complete.';

export const BATCH_ERROR_GUIDANCE =
  'Input validation rejects the call; runtime row errors stay indexed and isolated.';

export const CHEAP_VIEW_GUIDANCE =
  'Cheap views: compact schemas; localSearch resultView:"discovery"; content readers minify:"symbols".';
