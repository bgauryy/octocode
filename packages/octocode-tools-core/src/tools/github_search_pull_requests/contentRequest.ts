export type PartialContentRange = {
  file: string;
  additions?: number[];
  deletions?: number[];
};

export type PrContentSelector = {
  body?: boolean;
  changedFiles?: boolean;
  patches?: {
    mode: 'selected' | 'all';
    files?: string[];
    ranges?: PartialContentRange[];
  };
  comments?: {
    discussion?: boolean;
    reviewInline?: boolean;
    includeBots?: boolean;
    file?: string;
  };
  reviews?: boolean;
  commits?: {
    includeFiles?: boolean;
  };
};

export type PullRequestContentQuery = {
  content?: PrContentSelector;
};

export type NormalizedPrContentRequest = {
  body: boolean;
  changedFiles: boolean;
  patches: {
    mode: 'none' | 'selected' | 'all';
    files?: string[];
    ranges?: PartialContentRange[];
  };
  comments:
    | false
    | {
        discussion: boolean;
        reviewInline: boolean;
        includeBots: boolean;
        file?: string;
      };
  reviews: boolean;
  commits:
    | false
    | {
        list: boolean;
        includeFiles: boolean;
      };
};

function normalizePatches(
  content?: PrContentSelector
): NormalizedPrContentRequest['patches'] {
  const patchSelector = content?.patches;
  if (patchSelector?.mode) {
    return {
      mode: patchSelector.mode,
      ...(patchSelector.files ? { files: patchSelector.files } : {}),
      ...(patchSelector.ranges ? { ranges: patchSelector.ranges } : {}),
    };
  }
  return { mode: 'none' };
}

function normalizeComments(
  content?: PrContentSelector
): NormalizedPrContentRequest['comments'] {
  const comments = content?.comments;
  if (comments) {
    return {
      discussion: comments.discussion ?? true,
      reviewInline: comments.reviewInline ?? true,
      includeBots: comments.includeBots ?? false,
      ...(comments.file ? { file: comments.file } : {}),
    };
  }
  return false;
}

function normalizeCommits(
  content?: PrContentSelector
): NormalizedPrContentRequest['commits'] {
  const commits = content?.commits;
  if (commits) {
    return {
      list: true,
      includeFiles: commits.includeFiles ?? false,
    };
  }
  return false;
}

export function normalizePullRequestContentRequest(
  query: PullRequestContentQuery
): NormalizedPrContentRequest {
  const { content } = query;
  const patches = normalizePatches(content);
  const comments = normalizeComments(content);
  const commits = normalizeCommits(content);

  return {
    body: content?.body ?? false,
    changedFiles: (content?.changedFiles ?? false) || patches.mode !== 'none',
    patches,
    comments,
    reviews: content?.reviews ?? false,
    commits,
  };
}

export function hasExpensiveContentRequest(
  request: NormalizedPrContentRequest
): boolean {
  return Boolean(
    request.body ||
    request.changedFiles ||
    request.patches.mode !== 'none' ||
    request.comments ||
    request.reviews ||
    request.commits
  );
}
