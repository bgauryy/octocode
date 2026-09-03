import { assertContextSegmentAuthority, contentDigest, type ContextSegmentV1 } from '@octocodeai/octocode-awareness';

export interface ContextSegmentInput {
  id: string;
  content: string;
  kind: ContextSegmentV1['kind'];
  origin: string;
  authority: ContextSegmentV1['authority'];
  scope: ContextSegmentV1['scope'];
  visibility: ContextSegmentV1['visibility'];
  rehydrate: ContextSegmentV1['rehydrate'];
  tokenBudget?: number;
}

export interface AssembledContextV1 {
  version: 1;
  content: string;
  manifest: ContextSegmentV1[];
}

export const INITIAL_CONTEXT_TOKEN_BUDGET = 80_000;
export const PROVIDER_CONTEXT_TOKEN_BUDGET = 120_000;

export function estimateContextTokens(content: string): number {
  return Math.ceil(content.length / 4);
}

export function assertContextTokenBudget(label: string, contentChars: number, tokenBudget: number): number {
  const estimatedTokens = Math.ceil(contentChars / 4);
  if (estimatedTokens > tokenBudget) {
    throw new Error(`${label} exceeds total token budget ${tokenBudget} (estimated ${estimatedTokens})`);
  }
  return estimatedTokens;
}

export function contextSegmentFromInput(input: ContextSegmentInput): ContextSegmentV1 {
  if (!input.id.trim()) throw new Error('context segment id is required');
  const estimatedTokens = estimateContextTokens(input.content);
  if (input.tokenBudget !== undefined && estimatedTokens > input.tokenBudget) {
    throw new Error(`context segment ${input.id} exceeds token budget ${input.tokenBudget} (estimated ${estimatedTokens})`);
  }
  return assertContextSegmentAuthority({
    version: 1,
    id: input.id,
    kind: input.kind,
    origin: input.origin,
    authority: input.authority,
    digest: contentDigest(input.content),
    scope: input.scope,
    visibility: input.visibility,
    rehydrate: input.rehydrate,
    ...(input.tokenBudget === undefined ? {} : { tokenBudget: input.tokenBudget }),
  });
}

export function assembleContextSegments(
  inputs: ContextSegmentInput[],
  options: { totalTokenBudget?: number } = {},
): AssembledContextV1 {
  const seen = new Set<string>();
  const nonEmpty = inputs.filter((input) => input.content.trim().length > 0);
  let totalEstimatedTokens = 0;
  const manifest = nonEmpty.map((input) => {
    if (!input.id.trim() || seen.has(input.id)) throw new Error(`context segment id must be unique: ${input.id}`);
    seen.add(input.id);
    const estimatedTokens = estimateContextTokens(input.content);
    totalEstimatedTokens += estimatedTokens;
    return contextSegmentFromInput(input);
  });
  if (options.totalTokenBudget !== undefined && totalEstimatedTokens > options.totalTokenBudget) {
    throw new Error(`context assembly exceeds total token budget ${options.totalTokenBudget} (estimated ${totalEstimatedTokens})`);
  }
  return { version: 1, content: nonEmpty.map((input) => input.content).join('\n\n'), manifest };
}
