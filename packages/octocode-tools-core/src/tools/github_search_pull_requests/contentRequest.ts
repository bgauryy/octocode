export type PartialContentRange = {
  file: string;
  additions?: number[];
  deletions?: number[];
};

export type PrContentSelector = {
  body?: boolean;
  changedFiles?: boolean;
  patches?: {
    mode?: 'none' | 'selected' | 'all';
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
    list?: boolean;
    includeFiles?: boolean;
  };
};

export type PullRequestContentQuery = {
  content?: PrContentSelector;
  reviewMode?: 'full';
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
  reviewMode?: 'full';
};

function normalizePatches(
  content?: PrContentSelector,
  reviewMode?: 'full'
): NormalizedPrContentRequest['patches'] {
  const patchSelector = content?.patches;
  if (patchSelector?.mode) {
    return {
      mode: patchSelector.mode,
      ...(patchSelector.files ? { files: patchSelector.files } : {}),
      ...(patchSelector.ranges ? { ranges: patchSelector.ranges } : {}),
    };
  }
  if (reviewMode === 'full') return { mode: 'all' };
  return { mode: 'none' };
}

function normalizeComments(
  content?: PrContentSelector,
  reviewMode?: 'full'
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
  if (reviewMode === 'full') {
    return {
      discussion: true,
      reviewInline: true,
      includeBots: false,
    };
  }
  return false;
}

function normalizeCommits(
  content?: PrContentSelector,
  reviewMode?: 'full'
): NormalizedPrContentRequest['commits'] {
  const commits = content?.commits;
  if (commits) {
    return {
      list: commits.list ?? true,
      includeFiles: commits.includeFiles ?? false,
    };
  }
  if (reviewMode === 'full') {
    return { list: true, includeFiles: false };
  }
  return false;
}

export function normalizePullRequestContentRequest(
  query: PullRequestContentQuery
): NormalizedPrContentRequest {
  const { content, reviewMode } = query;
  const patches = normalizePatches(content, reviewMode);
  const comments = normalizeComments(content, reviewMode);
  const commits = normalizeCommits(content, reviewMode);
  const full = reviewMode === 'full';

  return {
    body: content?.body ?? full,
    changedFiles: (content?.changedFiles ?? full) || patches.mode !== 'none',
    patches,
    comments,
    reviews: content?.reviews ?? full,
    commits,
    ...(reviewMode ? { reviewMode } : {}),
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
