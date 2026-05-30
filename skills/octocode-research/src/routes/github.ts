/**
 * GitHub routes using route factory pattern.
 * 
 * @module routes/github
 */

import { Router } from 'express';
import {
  githubSearchCode,
  githubGetFileContent,
  githubSearchRepositories,
  githubViewRepoStructure,
  githubSearchPullRequests,
} from '../index.js';
import {
  githubSearchSchema,
  githubContentSchema,
  githubReposSchema,
  githubStructureSchema,
  githubPRsSchema,
} from '../validation/index.js';
import { ResearchResponse, QuickResult } from '../utils/responseBuilder.js';
import { withGitHubResilience } from '../utils/resilience.js';
import { createRouteHandler } from '../utils/routeFactory.js';
import { transformFileContentResponse } from '../utils/fileContentTransform.js';
import {
  safeString,
  safeNumber,
  safeArray,
  transformPagination,
} from '../utils/responseFactory.js';
import { isObject, hasProperty, hasNumberProperty } from '../types/guards.js';

export const githubRoutes = Router();

// GET /githubSearchCode - Search code on GitHub
githubRoutes.get(
  '/githubSearchCode',
  createRouteHandler({
    schema: githubSearchSchema,
    toolFn: githubSearchCode,
    toolName: 'githubSearchCode',
    resilience: withGitHubResilience,
    transform: (parsed, queries) => {
      const { data, hints, research } = parsed;
      const files = safeArray<Record<string, unknown>>(data, 'files');

      return ResearchResponse.searchResults({
        files: files.map((f) => {
          const textMatches = safeArray<string>(f, 'text_matches');
          const firstMatch = textMatches[0];
          return {
            path: safeString(f, 'path'),
            repo: hasProperty(f, 'repo') && typeof f.repo === 'string' ? f.repo : undefined,
            matches: textMatches.length,
            preview: typeof firstMatch === 'string' ? firstMatch.trim().slice(0, 200) : undefined,
          };
        }),
        totalMatches: isObject(data.pagination) ? safeNumber(data.pagination, 'totalMatches', 0) : 0,
        pagination: transformPagination(data.pagination),
        searchPattern: Array.isArray(queries[0]?.keywordsToSearch)
          ? queries[0].keywordsToSearch.join(' ')
          : undefined,
        mcpHints: hints,
        research,
      });
    },
  })
);

// GET /githubGetFileContent - Read file from GitHub
githubRoutes.get(
  '/githubGetFileContent',
  createRouteHandler({
    schema: githubContentSchema,
    toolFn: githubGetFileContent,
    toolName: 'githubGetFileContent',
    resilience: withGitHubResilience,
    transform: transformFileContentResponse,
  })
);

// GET /githubSearchRepositories - Search repositories
githubRoutes.get(
  '/githubSearchRepositories',
  createRouteHandler({
    schema: githubReposSchema,
    toolFn: githubSearchRepositories,
    toolName: 'githubSearchRepositories',
    resilience: withGitHubResilience,
    transform: (parsed) => {
      const { data, hints: mcpHints } = parsed;
      const repos = safeArray<Record<string, unknown>>(data, 'repositories');

      const summary = repos.length > 0
        ? `Found ${repos.length} repositories:\n` +
          repos
            .slice(0, 10)
            .map((r) =>
              `- ${safeString(r, 'owner')}/${safeString(r, 'repo')}${hasNumberProperty(r, 'stars') ? ` ⭐${r.stars}` : ''}\n  ${safeString(r, 'description', 'No description')}`
            )
            .join('\n')
        : 'No repositories found';

      const hints: string[] = [...mcpHints];
      return repos.length === 0
        ? QuickResult.empty(summary, hints.length > 0 ? hints : [
            'Try different search terms',
            'Use topicsToSearch for topic-based search',
          ])
        : QuickResult.success(summary, data, hints.length > 0 ? hints : [
            'Use githubViewRepoStructure to explore repo',
            'Use githubSearchCode to search within repo',
          ]);
    },
  })
);

// GET /githubViewRepoStructure - View repository structure
githubRoutes.get(
  '/githubViewRepoStructure',
  createRouteHandler({
    schema: githubStructureSchema,
    toolFn: githubViewRepoStructure,
    toolName: 'githubViewRepoStructure',
    resilience: withGitHubResilience,
    transform: (parsed, queries) => {
      const { data, hints, research } = parsed;
      const structure = isObject(data.structure) ? data.structure : {};
      const rootEntry = isObject(structure['.']) ? structure['.'] as { files?: string[]; folders?: string[] } : { files: [], folders: [] };
      const summary = isObject(data.summary) ? data.summary : {};

      return ResearchResponse.repoStructure({
        path: queries[0]?.path || '/',
        structure: {
          files: Array.isArray(rootEntry.files) ? rootEntry.files : [],
          folders: Array.isArray(rootEntry.folders) ? rootEntry.folders : [],
        },
        depth: hasNumberProperty(queries[0], 'depth') ? queries[0].depth : undefined,
        totalFiles: hasNumberProperty(summary, 'totalFiles') ? summary.totalFiles : undefined,
        totalFolders: hasNumberProperty(summary, 'totalFolders') ? summary.totalFolders : undefined,
        owner: queries[0]?.owner,
        repo: queries[0]?.repo,
        mcpHints: hints,
        research,
      });
    },
  })
);

// GET /githubSearchPullRequests - Search pull requests
githubRoutes.get(
  '/githubSearchPullRequests',
  createRouteHandler({
    schema: githubPRsSchema,
    toolFn: githubSearchPullRequests,
    toolName: 'githubSearchPullRequests',
    resilience: withGitHubResilience,
    transform: (parsed, queries) => {
      const { data, hints, research } = parsed;
      const prs = safeArray<Record<string, unknown>>(data, 'pull_requests');

      return ResearchResponse.pullRequests({
        prs: prs.map((pr) => ({
          number: safeNumber(pr, 'number', 0),
          title: safeString(pr, 'title'),
          state: safeString(pr, 'state'),
          author: hasProperty(pr, 'author') && typeof pr.author === 'string' ? pr.author : undefined,
          url: hasProperty(pr, 'url') && typeof pr.url === 'string' ? pr.url : undefined,
        })),
        repo: queries[0]?.owner && queries[0]?.repo
          ? `${queries[0].owner}/${queries[0].repo}`
          : undefined,
        pagination: transformPagination(data.pagination),
        mcpHints: hints,
        research,
      });
    },
  })
);
