import type { ParsedResponse } from './responseParser.js';
import {
  ResearchResponse,
  detectLanguageFromPath,
} from './responseBuilder.js';
import { safeString } from './responseFactory.js';
import {
  hasBooleanProperty,
  hasNumberProperty,
} from '../types/guards.js';

type FileContentQuery = {
  path?: string;
};

export function transformFileContentResponse(
  parsed: ParsedResponse,
  queries: FileContentQuery[]
): ReturnType<typeof ResearchResponse.fileContent> {
  const { data, hints, research } = parsed;
  const path = queries[0]?.path || '';

  return ResearchResponse.fileContent({
    path: safeString(data, 'path', path || 'unknown'),
    content: safeString(data, 'content'),
    lines: hasNumberProperty(data, 'startLine')
      ? {
          start: data.startLine,
          end: hasNumberProperty(data, 'endLine')
            ? data.endLine
            : data.startLine,
        }
      : undefined,
    language: detectLanguageFromPath(path),
    totalLines: hasNumberProperty(data, 'totalLines')
      ? data.totalLines
      : undefined,
    isPartial: hasBooleanProperty(data, 'isPartial')
      ? data.isPartial
      : undefined,
    mcpHints: hints,
    research,
  });
}
