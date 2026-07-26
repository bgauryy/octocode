import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import {
  validateToolPath,
  createErrorResult,
} from '../../utils/file/toolHelpers.js';
import { ToolErrors } from '../../errors/errorFactories.js';
import { buildNextPageContinuation } from '../../scheme/pagination.js';
import type { WithOptionalMeta } from '../../types/execution.js';
import { scanForDeadCode } from './deadCodeScan.js';
import type {
  DeadExportOutput,
  FindDeadCodeOutput,
  FindDeadCodeQuery as CoreFindDeadCodeQuery,
} from './scheme.js';

type FindDeadCodeQuery = WithOptionalMeta<CoreFindDeadCodeQuery>;

const DEFAULT_ITEMS_PER_PAGE = 100;

function paginate<T>(
  items: T[],
  page: number,
  itemsPerPage: number
): { pageItems: T[]; totalPages: number; hasMore: boolean } {
  const totalPages = Math.max(1, Math.ceil(items.length / itemsPerPage));
  const currentPage = Math.min(Math.max(1, page), totalPages);
  const start = (currentPage - 1) * itemsPerPage;
  const pageItems = items.slice(start, start + itemsPerPage);
  return { pageItems, totalPages, hasMore: currentPage < totalPages };
}

export async function findDeadCode(
  query: FindDeadCodeQuery
): Promise<FindDeadCodeOutput> {
  try {
    const pathValidation = validateToolPath(
      query,
      TOOL_NAMES.LOCAL_FIND_DEAD_CODE
    );
    if (!pathValidation.isValid) {
      return pathValidation.errorResult as FindDeadCodeOutput;
    }

    const result = scanForDeadCode(pathValidation.sanitizedPath, query);

    const itemsPerPage = query.itemsPerPage ?? DEFAULT_ITEMS_PER_PAGE;
    const page = query.page ?? 1;
    const { pageItems, totalPages, hasMore } = paginate<DeadExportOutput>(
      result.deadExports,
      page,
      itemsPerPage
    );

    const output: FindDeadCodeOutput = {
      path: query.path,
      filesScanned: result.filesScanned,
      filesSkipped: result.filesSkipped,
      entrypointsResolved: result.entrypointsResolved,
      deadExports: pageItems,
      deadClusters: result.deadClusters,
      pagination: {
        currentPage: page,
        totalPages,
        entriesPerPage: itemsPerPage,
        totalEntries: result.deadExports.length,
        hasMore,
      },
      ...(result.warnings.length > 0 ? { warnings: result.warnings } : {}),
    };

    if (hasMore) {
      output.next = {
        nextPage: buildNextPageContinuation(
          TOOL_NAMES.LOCAL_FIND_DEAD_CODE,
          { ...query, page: page + 1 },
          'Continue to the next page of dead-export candidates.'
        ),
      };
    }

    return output;
  } catch (error) {
    const toolError = ToolErrors.toolExecutionFailed(
      TOOL_NAMES.LOCAL_FIND_DEAD_CODE,
      error instanceof Error ? error : new Error(String(error))
    );
    return createErrorResult(toolError, query, {
      toolName: TOOL_NAMES.LOCAL_FIND_DEAD_CODE,
    }) as FindDeadCodeOutput;
  }
}
