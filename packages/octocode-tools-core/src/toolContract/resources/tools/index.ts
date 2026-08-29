import type { ToolSpec } from '../../types.js';

import { ghSearchCode } from './ghSearchCode.js';
import { ghGetFileContent } from './ghGetFileContent.js';
import { ghViewRepoStructure } from './ghViewRepoStructure.js';
import { ghSearchRepos } from './ghSearchRepos.js';
import { ghSearchPullRequests } from './ghSearchPullRequests.js';
import { ghSearchIssues } from './ghSearchIssues.js';
import { ghSearchCommits } from './ghSearchCommits.js';
import { ghListReleases } from './ghListReleases.js';
import { ghSearchDiscussions } from './ghSearchDiscussions.js';
import { npmSearch } from './npmSearch.js';
import { ghCloneRepo } from './ghCloneRepo.js';
import { localSearchCode } from './localSearchCode.js';
import { localViewStructure } from './localViewStructure.js';
import { localFindFiles } from './localFindFiles.js';
import { localAnalyzeGraph } from './localAnalyzeGraph.js';
import { localGetFileContent } from './localGetFileContent.js';
import { lspGetSemantics } from './lspGetSemantics.js';

export const toolSpecs = {
  ghSearchCode,
  ghGetFileContent,
  ghViewRepoStructure,
  ghSearchRepos,
  ghSearchPullRequests,
  ghSearchIssues,
  ghSearchCommits,
  ghListReleases,
  ghSearchDiscussions,
  npmSearch,
  ghCloneRepo,
  localSearchCode,
  localViewStructure,
  localFindFiles,
  localAnalyzeGraph,
  localGetFileContent,
  lspGetSemantics,
} as const;

export const toolSpecList: readonly ToolSpec[] = Object.values(toolSpecs);
