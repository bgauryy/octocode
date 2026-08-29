import { z } from 'zod';

import type { ToolSpec } from '../../types.js';
import {
  buildObject,
  DEFAULT_PR_ITEMS_PER_PAGE,
  defineTool,
  intRange,
  MAX_PR_ITEMS_PER_PAGE,
  metaFields,
  pageNumber,
} from './_toolkit.js';

export const ghListReleases: ToolSpec = defineTool({
  name: 'ghListReleases',
  type: 'Github',
  shortDescription:
    "List a GitHub repository's releases and surface the latest stable release.",
  instructions: `See release history (tagName, publishedAt, prerelease) and the newest stable release — not PRs (ghSearchPullRequests) or commits (ghSearchCommits). Each entry carries prerelease: true/false — check it yourself before treating a tag as stable; don't assume list order alone settles it.
owner+repo identify the repo; page/itemsPerPage walk the list. Follow a tag into ghGetFileContent/ghViewRepoStructure at that ref.`,
  schema: {
    owner: 'Repository owner.',
    repo: 'Repository name.',
    itemsPerPage: 'Releases returned per page (walk with page).',
    includeAssets:
      "Attach each release's downloadable assets (name, size, download count, url).",
  },
});

export const ListReleasesQuerySchema = buildObject(ghListReleases.schema, {
  ...metaFields,
  owner: z.string(),
  repo: z.string(),
  page: pageNumber(),
  itemsPerPage: intRange(1, MAX_PR_ITEMS_PER_PAGE).default(
    DEFAULT_PR_ITEMS_PER_PAGE
  ),
  includeAssets: z.boolean().optional(),
});
