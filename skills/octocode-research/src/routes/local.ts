/**
 * Local filesystem routes using route factory pattern.
 * 
 * @module routes/local
 */

import { Router } from 'express';
import {
  localSearchCode,
  localGetFileContent,
  localFindFiles,
  localViewStructure,
} from '../index.js';
import {
  localSearchSchema,
  localContentSchema,
  localFindSchema,
  localStructureSchema,
} from '../validation/index.js';
import { ResearchResponse } from '../utils/responseBuilder.js';
import { withLocalResilience } from '../utils/resilience.js';
import { createRouteHandler } from '../utils/routeFactory.js';
import { transformFileContentResponse } from '../utils/fileContentTransform.js';
import {
  safeString,
  safeNumber,
  safeArray,
  extractMatchLocations,
  transformPagination,
} from '../utils/responseFactory.js';
import { isObject, hasNumberProperty } from '../types/guards.js';

export const localRoutes = Router();

// GET /localSearchCode - Search code with ripgrep
localRoutes.get(
  '/localSearchCode',
  createRouteHandler({
    schema: localSearchSchema,
    toolFn: localSearchCode,
    toolName: 'localSearchCode',
    resilience: withLocalResilience,
    transform: (parsed, queries) => {
      const { data, hints, research } = parsed;
      const files = safeArray<Record<string, unknown>>(data, 'files');
      const pagination = isObject(data.pagination) ? data.pagination : {};

      return ResearchResponse.searchResults({
        files: files.map((f) => {
          const matchesArray = safeArray<Record<string, unknown>>(f, 'matches');
          const firstMatch = matchesArray[0];
          return {
            path: safeString(f, 'path'),
            matches: hasNumberProperty(f, 'matchCount') ? f.matchCount : matchesArray.length,
            line: isObject(firstMatch) && hasNumberProperty(firstMatch, 'line') ? firstMatch.line : undefined,
            preview: isObject(firstMatch) && typeof firstMatch.value === 'string' ? firstMatch.value.trim() : undefined,
            allMatches: extractMatchLocations(matchesArray),
          };
        }),
        totalMatches: safeNumber(data, 'totalMatches', 0),
        pagination: transformPagination(pagination),
        searchPattern: queries[0]?.pattern,
        mcpHints: hints,
        research,
      });
    },
  })
);

// GET /localGetFileContent - Read file contents
localRoutes.get(
  '/localGetFileContent',
  createRouteHandler({
    schema: localContentSchema,
    toolFn: localGetFileContent,
    toolName: 'localGetFileContent',
    resilience: withLocalResilience,
    transform: transformFileContentResponse,
  })
);

// GET /localFindFiles - Find files by metadata
localRoutes.get(
  '/localFindFiles',
  createRouteHandler({
    schema: localFindSchema,
    toolFn: localFindFiles,
    toolName: 'localFindFiles',
    resilience: withLocalResilience,
    transform: (parsed) => {
      const { data, hints } = parsed;
      const files = safeArray<Record<string, unknown>>(data, 'files');
      
      const summary = files.length > 0
        ? `Found ${files.length} files:\n` +
          files
            .slice(0, 20)
            .map((f) =>
              `- ${safeString(f, 'path')}${hasNumberProperty(f, 'size') ? ` (${formatSize(f.size)})` : ''}`
            )
            .join('\n')
        : 'No files found';

      const defaultHints = ['Use localGetFileContent to read file contents', 'Use localSearchCode to search within files'];
      const emptyHints = ['Try different name pattern', 'Check path filter', 'Use -iname for case-insensitive search'];

      return files.length === 0
        ? { content: [{ type: 'text' as const, text: summary }], structuredContent: { status: 'empty', hints: emptyHints, data } }
        : { content: [{ type: 'text' as const, text: summary }], structuredContent: { status: 'hasResults', hints: hints.length > 0 ? hints : defaultHints, data } };
    },
  })
);

// GET /localViewStructure - View directory structure
localRoutes.get(
  '/localViewStructure',
  createRouteHandler({
    schema: localStructureSchema,
    toolFn: localViewStructure,
    toolName: 'localViewStructure',
    resilience: withLocalResilience,
    transform: (parsed, queries) => {
      const { data, hints, research } = parsed;
      const structuredOutput = safeString(data, 'structuredOutput');
      const files: string[] = [];
      const folders: string[] = [];

      // Extract files and folders from output
      const lines = structuredOutput.split('\n');
      for (const line of lines) {
        if (line.includes('[FILE]')) {
          const match = line.match(/\[FILE\]\s+(.+?)(?:\s+\(|$)/);
          if (match) files.push(match[1].trim());
        } else if (line.includes('[DIR]')) {
          const match = line.match(/\[DIR\]\s+(.+?)(?:\s*$)/);
          if (match) folders.push(match[1].trim());
        }
      }

      return ResearchResponse.repoStructure({
        path: queries[0]?.path || '.',
        structure: { files, folders },
        depth: hasNumberProperty(queries[0], 'depth') ? queries[0].depth : undefined,
        totalFiles: hasNumberProperty(data, 'totalFiles') ? data.totalFiles : undefined,
        totalFolders: hasNumberProperty(data, 'totalDirectories') ? data.totalDirectories : undefined,
        mcpHints: hints,
        research,
      });
    },
  })
);

// Helper: Format file size
function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes}B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)}KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)}MB`;
}
