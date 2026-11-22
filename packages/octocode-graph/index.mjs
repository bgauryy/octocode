import { octocode } from './skills/octocode-graph/scripts/lib.mjs';

export { startMcp } from './mcp.mjs';

/** Create a native snapshot. The response carries its graph ID or continuation. */
export function graphBuild(input) {
  return octocode(['graphBuild', '--json', JSON.stringify(input)], { partial: true });
}

/** Query a native snapshot or follow a returned cursor without reshaping its evidence. */
export function graphQuery(input) {
  return octocode(['graphQuery', '--json', JSON.stringify(input)], { partial: true });
}
