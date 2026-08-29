import { z } from 'zod';

import type { ToolSpec } from '../../types.js';
import {
  buildObject,
  DEFAULT_PR_ITEMS_PER_PAGE,
  defineTool,
  intRange,
  MAX_PR_ITEMS_PER_PAGE,
  metaFields,
} from './_toolkit.js';

export const ghSearchDiscussions: ToolSpec = defineTool({
  name: 'ghSearchDiscussions',
  type: 'Github',
  shortDescription:
    "Search a GitHub repository's Discussions (Q&A, announcements, ideas).",
  instructions: `Community/maintainer discussion — questions, RFCs, announcements — not code/PRs/issues/commits. Discussions is an opt-in repo feature, unlike Issues/PRs — most repos never enable it, so an empty result usually means it's off, not that nothing was discussed.
owner+repo identify the repo; keywordsToSearch matches title/body (omit to list newest); itemsPerPage sizes the page, after continues from a prior nextCursor. Backed by GitHub's GraphQL API, so pagination is cursor-based (after/nextCursor) — unlike this toolkit's other GitHub tools, which page by number.`,
  schema: {
    owner: 'Repository owner.',
    repo: 'Repository name.',
    keywordsToSearch:
      'Terms matched in discussion title/body; omit to list newest discussions.',
    itemsPerPage: 'Discussions returned per page (continue with after).',
    after: "Pagination cursor from a previous response's nextCursor.",
  },
});

export const SearchDiscussionsQuerySchema = buildObject(
  ghSearchDiscussions.schema,
  {
    ...metaFields,
    owner: z.string(),
    repo: z.string(),
    keywordsToSearch: z.array(z.string()).optional(),
    itemsPerPage: intRange(1, MAX_PR_ITEMS_PER_PAGE).default(
      DEFAULT_PR_ITEMS_PER_PAGE
    ),
    after: z.string().optional(),
  }
);
