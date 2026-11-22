import { runGraphCli } from './bin/octocode-graph.mjs';

/** Serve the graph MCP over this process's stdin and stdout. */
export function startMcp() {
  return runGraphCli(['mcp']);
}
