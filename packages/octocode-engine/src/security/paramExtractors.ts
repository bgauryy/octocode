export interface ResearchFields {
  goal?: string;
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
  if (typeof query.goal === 'string' && query.goal) {
    fields.goal = query.goal;
  }
  if (typeof query.reasoning === 'string' && query.reasoning) {
    fields.reasoning = query.reasoning;
  }
  return fields;
}

export function extractResearchFields(
  params: Record<string, unknown>
): ResearchFields {
  const queries = getQueriesArray(params);

  if (!queries) {
    return extractResearchFieldsFromQuery(params);
  }

  const goals = new Set<string>();
  const reasonings = new Set<string>();

  for (const query of queries) {
    const fields = extractResearchFieldsFromQuery(query);
    if (fields.goal) goals.add(fields.goal);
    if (fields.reasoning) reasonings.add(fields.reasoning);
  }

  return {
    ...(goals.size > 0 && { goal: Array.from(goals).join('; ') }),
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
