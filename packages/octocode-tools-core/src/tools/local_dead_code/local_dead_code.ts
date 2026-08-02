import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import { LSP_GET_SEMANTICS_TOOL_NAME } from '../toolNames.js';
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
): {
  pageItems: T[];
  /** The page actually returned — the requested page clamped into range. */
  currentPage: number;
  totalPages: number;
  hasMore: boolean;
  /** True when the requested page exceeded totalPages and was clamped. */
  outOfRange: boolean;
} {
  const totalPages = Math.max(1, Math.ceil(items.length / itemsPerPage));
  const currentPage = Math.min(Math.max(1, page), totalPages);
  const start = (currentPage - 1) * itemsPerPage;
  const pageItems = items.slice(start, start + itemsPerPage);
  return {
    pageItems,
    currentPage,
    totalPages,
    hasMore: currentPage < totalPages,
    outOfRange: page > totalPages,
  };
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
    const { pageItems, currentPage, totalPages, hasMore, outOfRange } =
      paginate<DeadExportOutput>(result.deadExports, page, itemsPerPage);

    // The envelope must report the page actually returned (the clamp), never
    // echo an out-of-range request as if it were served — page:99 of a 2-page
    // result returns page 2's items and must say so.
    const warnings = [
      ...result.warnings,
      ...(outOfRange
        ? [
            `page:${page} is out of range (only ${totalPages} page(s), ${result.deadExports.length} total candidates) — returned page ${currentPage} instead.`,
          ]
        : []),
    ];

    const output: FindDeadCodeOutput = {
      path: query.path,
      filesScanned: result.filesScanned,
      filesSkipped: result.filesSkipped,
      entrypointsResolved: result.entrypointsResolved,
      deadExports: pageItems,
      deadClusters: result.deadClusters,
      pagination: {
        currentPage,
        totalPages,
        entriesPerPage: itemsPerPage,
        totalEntries: result.deadExports.length,
        hasMore,
        ...(outOfRange ? { outOfRange: true } : {}),
      },
      ...(warnings.length > 0 ? { warnings } : {}),
      ...(result.confidence ? { confidence: result.confidence } : {}),
    };

    if (hasMore) {
      output.next = {
        nextPage: buildNextPageContinuation(
          TOOL_NAMES.LOCAL_FIND_DEAD_CODE,
          { ...query, page: currentPage + 1 },
          'Continue to the next page of dead-export candidates.'
        ),
      };
    }

    // Candidate → proof escalation: every dead export is a CANDIDATE, not a
    // verdict. Emit a prefilled lspGetSemantics references query for the
    // first candidate so the verification loop needs zero query authoring —
    // substitute uri/symbolName/lineHint per candidate from `deadExports`.
    const firstCandidate = pageItems[0];
    if (firstCandidate) {
      const root = pathValidation.sanitizedPath.replace(/\/+$/, '');
      output.next = {
        ...output.next,
        verifyReferences: {
          tool: LSP_GET_SEMANTICS_TOOL_NAME,
          query: {
            type: 'references',
            uri: `${root}/${firstCandidate.file}`,
            symbolName: firstCandidate.name,
            lineHint: firstCandidate.line,
            includeDeclaration: false,
            groupByFile: true,
          },
          why: `Verify candidate "${firstCandidate.name}" before any delete claim — zero references = confirmed dead. Repeat per candidate using each entry's file/name/line; start with viaHeuristic:"reexport-chain" candidates (most fragile).`,
          confidence: 'high',
        },
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
