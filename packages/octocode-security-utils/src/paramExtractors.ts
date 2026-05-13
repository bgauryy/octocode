/**
 * Parameter extraction utilities for MCP tool queries.
 *
 * Extracts research metadata (goals, reasoning) and repository ownership
 * from tool parameters — both single-operation and batched (queries[]) formats.
 */

export interface ResearchFields {
  mainResearchGoal?: string;
  researchGoal?: string;
  reasoning?: string;
}

function getQueriesArray(
  params: Record<string, unknown>
): Array<Record<string, unknown>> | undefined {
  const queries = params.queries;
  if (queries && Array.isArray(queries) && queries.length > 0) {
    return queries as Array<Record<string, unknown>>;
  }
  return undefined;
}

function extractResearchFieldsFromQuery(
  query: Record<string, unknown>
): ResearchFields {
  const fields: ResearchFields = {};
  if (typeof query.mainResearchGoal === 'string' && query.mainResearchGoal) {
    fields.mainResearchGoal = query.mainResearchGoal;
  }
  if (typeof query.researchGoal === 'string' && query.researchGoal) {
    fields.researchGoal = query.researchGoal;
  }
  if (typeof query.reasoning === 'string' && query.reasoning) {
    fields.reasoning = query.reasoning;
  }
  return fields;
}

/**
 * Extract research metadata (goals, reasoning) from tool parameters.
 *
 * @example
 * ```ts
 * extractResearchFields({
 *   queries: [{ researchGoal: 'find auth flow', reasoning: 'tracing login' }]
 * });
 * // → { researchGoal: 'find auth flow', reasoning: 'tracing login' }
 * ```
 */
export function extractResearchFields(
  params: Record<string, unknown>
): ResearchFields {
  const queries = getQueriesArray(params);

  if (!queries) {
    return extractResearchFieldsFromQuery(params);
  }

  const mainGoals = new Set<string>();
  const goals = new Set<string>();
  const reasonings = new Set<string>();

  for (const query of queries) {
    const fields = extractResearchFieldsFromQuery(query);
    if (fields.mainResearchGoal) mainGoals.add(fields.mainResearchGoal);
    if (fields.researchGoal) goals.add(fields.researchGoal);
    if (fields.reasoning) reasonings.add(fields.reasoning);
  }

  return {
    ...(mainGoals.size > 0 && {
      mainResearchGoal: Array.from(mainGoals).join('; '),
    }),
    ...(goals.size > 0 && { researchGoal: Array.from(goals).join('; ') }),
    ...(reasonings.size > 0 && {
      reasoning: Array.from(reasonings).join('; '),
    }),
  };
}

function extractRepoOwnerFromQuery(query: Record<string, unknown>): string[] {
  const repository =
    typeof query.repository === 'string' ? query.repository : undefined;

  if (repository && repository.includes('/')) {
    return [repository];
  }

  const repo = typeof query.repo === 'string' ? query.repo : undefined;
  const owner = typeof query.owner === 'string' ? query.owner : undefined;

  if (owner && repo) {
    return [`${owner}/${repo}`];
  }
  if (owner) {
    return [owner];
  }
  return [];
}

/**
 * Extract repository identifiers (owner/repo) from tool parameters.
 *
 * @example
 * ```ts
 * extractRepoOwnerFromParams({ owner: 'facebook', repo: 'react' });
 * // → ['facebook/react']
 * ```
 */
export function extractRepoOwnerFromParams(
  params: Record<string, unknown>
): string[] {
  const queries = getQueriesArray(params);

  if (!queries) {
    return extractRepoOwnerFromQuery(params);
  }

  const repoSet = new Set<string>();
  for (const query of queries) {
    for (const repo of extractRepoOwnerFromQuery(query)) {
      repoSet.add(repo);
    }
  }
  return Array.from(repoSet);
}
