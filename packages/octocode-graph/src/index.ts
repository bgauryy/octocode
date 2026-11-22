import { octocode } from './runtime/lib.mjs';

import type {
  GraphBuildInput,
  GraphBuildOutput,
  GraphQueryInput,
  GraphQueryOutput,
} from './contract.generated.js';
export type {
  GraphBuildInput,
  GraphBuildOutput,
  GraphQueryInput,
  GraphQueryOutput,
} from './contract.generated.js';
export { startMcp } from './mcp.js';

/** The native engine validates the input and preserves the full response envelope. */
export function graphBuild(input: GraphBuildInput): Promise<GraphBuildOutput> {
  return octocode(['graphBuild', '--json', JSON.stringify(input)], {
    partial: true,
  }) as Promise<GraphBuildOutput>;
}

export function graphQuery(input: GraphQueryInput): Promise<GraphQueryOutput> {
  return octocode(['graphQuery', '--json', JSON.stringify(input)], {
    partial: true,
  }) as Promise<GraphQueryOutput>;
}
