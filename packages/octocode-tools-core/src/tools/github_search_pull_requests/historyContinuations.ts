type QueryRecord = Record<string, unknown>;

function compact(value: QueryRecord): QueryRecord {
  return Object.fromEntries(
    Object.entries(value).filter(([, entry]) => entry !== undefined)
  );
}

function pick(query: QueryRecord, fields: readonly string[]): QueryRecord {
  return compact(
    Object.fromEntries(fields.map(field => [field, query[field]]))
  );
}

function isRecord(value: unknown): value is QueryRecord {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function trueFlag(value: unknown): true | undefined {
  return value === true ? true : undefined;
}

function sanitizePatchRanges(value: unknown): QueryRecord[] | undefined {
  if (!Array.isArray(value)) return undefined;
  const ranges = value.flatMap(entry => {
    if (!isRecord(entry) || typeof entry.file !== 'string') return [];
    const additions = Array.isArray(entry.additions)
      ? entry.additions.filter(item => typeof item === 'number')
      : undefined;
    const deletions = Array.isArray(entry.deletions)
      ? entry.deletions.filter(item => typeof item === 'number')
      : undefined;
    return [
      compact({
        file: entry.file,
        ...(additions?.length ? { additions } : {}),
        ...(deletions?.length ? { deletions } : {}),
      }),
    ];
  });
  return ranges.length > 0 ? ranges : undefined;
}

/** Rebuild a content selector using only fields accepted by the public schema. */
export function sanitizePullRequestContent(
  value: unknown
): QueryRecord | undefined {
  if (!isRecord(value)) return undefined;
  const comments = isRecord(value.comments)
    ? compact({
        discussion: trueFlag(value.comments.discussion),
        reviewInline: trueFlag(value.comments.reviewInline),
        includeBots: trueFlag(value.comments.includeBots),
        file:
          typeof value.comments.file === 'string'
            ? value.comments.file
            : undefined,
      })
    : undefined;
  const commits = isRecord(value.commits)
    ? compact({ includeFiles: trueFlag(value.commits.includeFiles) })
    : undefined;
  const patches = isRecord(value.patches)
    ? compact({
        mode:
          value.patches.mode === 'all' || value.patches.mode === 'selected'
            ? value.patches.mode
            : undefined,
        files: Array.isArray(value.patches.files)
          ? value.patches.files.filter(item => typeof item === 'string')
          : undefined,
        ranges: sanitizePatchRanges(value.patches.ranges),
      })
    : undefined;
  const content = compact({
    body: trueFlag(value.body),
    changedFiles: trueFlag(value.changedFiles),
    ...(patches && Object.keys(patches).length > 0 ? { patches } : {}),
    ...(comments && Object.keys(comments).length > 0 ? { comments } : {}),
    reviews: trueFlag(value.reviews),
    // An empty commits selector requests summaries and must survive paging.
    ...(commits ? { commits } : {}),
  });
  return Object.keys(content).length > 0 ? content : undefined;
}

/** Build an executable pull-request continuation from an internal executor query. */
export function publicPullRequestContinuationQuery(
  query: QueryRecord,
  number: number,
  patch: QueryRecord = {}
): QueryRecord {
  const merged = { ...query, ...patch };
  const originalContent = sanitizePullRequestContent(query.content);
  const patchContent = sanitizePullRequestContent(patch.content);
  const content =
    originalContent || patchContent
      ? { ...(patchContent ?? {}), ...(originalContent ?? {}) }
      : undefined;
  return compact({
    operation: 'pullRequest',
    owner: query.owner,
    repo: query.repo,
    number,
    ...pick(merged, [
      'pageSize',
      'filePage',
      'commentPage',
      'commitPage',
      'reviewPage',
      'collectionPages',
      'charOffset',
      'commentBodyOffset',
      'charLength',
      'matchString',
      'minify',
    ]),
    ...(content ? { content } : {}),
  });
}

/** Build an executable exact-commit continuation from transport-enriched input. */
export function publicCommitContinuationQuery(
  query: QueryRecord,
  patch: QueryRecord = {}
): QueryRecord {
  const merged = { ...query, ...patch };
  return compact({
    operation: 'commit',
    ...pick(merged, [
      'owner',
      'repo',
      'ref',
      'includeDiff',
      'fileBatch',
      'path',
      'filePage',
      'pageSize',
      'charOffset',
      'charLength',
    ]),
  });
}

const SEARCH_FIELDS = {
  pullRequests: [
    'keywords',
    'owner',
    'repo',
    'concise',
    'state',
    'assignee',
    'author',
    'commenter',
    'mentions',
    'review-requested',
    'reviewed-by',
    'label',
    'checks',
    'review',
    'head',
    'base',
    'created',
    'updated',
    'closed',
    'merged-at',
    'comments',
    'reactions',
    'draft',
    'match',
    'sort',
    'order',
    'archived',
    'minify',
    'pageSize',
  ],
  issues: [
    'owner',
    'repo',
    'keywords',
    'concise',
    'state',
    'author',
    'assignee',
    'commenter',
    'mentions',
    'label',
    'created',
    'updated',
    'closed',
    'comments',
    'reactions',
    'match',
    'sort',
    'order',
    'archived',
    'minify',
    'pageSize',
  ],
  commits: [
    'keywords',
    'owner',
    'repo',
    'path',
    'since',
    'until',
    'branch',
    'author',
    'committer',
    'pageSize',
  ],
} as const;

/** Build one runnable next-page query without executor-only transport fields. */
export function publicSearchContinuationQuery(
  query: QueryRecord,
  operation: keyof typeof SEARCH_FIELDS,
  page: number
): QueryRecord {
  return {
    operation,
    ...pick(query, SEARCH_FIELDS[operation]),
    page,
  };
}
