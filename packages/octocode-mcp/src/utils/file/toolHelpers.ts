import path from 'path';
import { pathValidator } from 'octocode-security-utils/pathValidator';
import { ToolErrors } from '../../errors/errorFactories.js';
import type { BaseQueryLocal } from '@octocodeai/octocode-core/extra-types';
type PartialBaseQueryLocal = Partial<BaseQueryLocal>;
import {
  createErrorResult,
  type UnifiedErrorResult,
} from '../response/error.js';
import { getConfigSync } from 'octocode-shared';

type LocalErrorResult = UnifiedErrorResult;

export { createErrorResult };

interface ToolPathValidationResult {
  isValid: boolean;
  errorResult?: LocalErrorResult;
  sanitizedPath?: string;
}

function getPathErrorHints(
  inputPath: string,
  errorMessage: string | undefined,
  cwd: string,
  resolvedPath: string
): string[] {
  const hints: string[] = [];

  const resolvedInfo =
    inputPath !== resolvedPath ? ` (resolved to: ${resolvedPath})` : '';
  hints.push(`CWD: ${cwd}`);

  if (errorMessage?.includes('outside allowed')) {
    hints.push(
      `Fix: Use absolute path within workspace, e.g. path="${cwd}/..."${resolvedInfo}`
    );
  } else if (errorMessage?.includes('Permission denied')) {
    hints.push('Fix: Check file/directory permissions');
  } else if (
    errorMessage?.includes('Symlink') ||
    errorMessage?.includes('symlink')
  ) {
    hints.push('Fix: Symlink target may be outside allowed directories');
  } else if (
    errorMessage?.includes('ENOENT') ||
    errorMessage?.includes('not found')
  ) {
    hints.push(
      `Fix: Path not found. Use absolute path, e.g. path="${cwd}/..."${resolvedInfo}`
    );
  }

  return hints;
}

export function validateToolPath(
  query: PartialBaseQueryLocal & { path?: string },
  toolName: string
): ToolPathValidationResult {
  if (!query.path?.trim()) {
    const toolError = ToolErrors.pathValidationFailed('', 'path is required');
    return {
      isValid: false,
      errorResult: createErrorResult(toolError, query, { toolName }),
    };
  }
  const cwd =
    process.env.WORKSPACE_ROOT?.trim() ||
    getConfigSync().local.workspaceRoot ||
    process.cwd();
  const inputPath = query.path.replace(/^file:\/\//, '');
  const resolvedPath = path.isAbsolute(inputPath)
    ? inputPath
    : path.resolve(cwd, inputPath);

  const validation = pathValidator.validate(resolvedPath);

  if (!validation.isValid) {
    const toolError = ToolErrors.pathValidationFailed(
      query.path,
      validation.error
    );

    const pathHints = getPathErrorHints(
      query.path,
      validation.error,
      cwd,
      resolvedPath
    );

    return {
      isValid: false,
      errorResult: createErrorResult(toolError, query, {
        toolName,
        hintContext: {
          errorType: 'permission',
          path: query.path,
          originalError: validation.error,
        },
        extra: {
          cwd,
          resolvedPath,
        },
        customHints: pathHints,
      }),
    };
  }

  return { isValid: true, sanitizedPath: validation.sanitizedPath };
}

interface LargeOutputSafetyOptions {
  threshold?: number;
  itemType?: string;
  detailed?: boolean;
}

interface LargeOutputSafetyResult {
  shouldBlock: boolean;
  errorCode?: string;
  hints?: string[];
}

export function checkLargeOutputSafety(
  itemCount: number,
  hasCharLength: boolean,
  options: LargeOutputSafetyOptions = {}
): LargeOutputSafetyResult {
  const { threshold = 100, itemType = 'item', detailed = false } = options;

  if (hasCharLength) {
    return { shouldBlock: false };
  }

  if (itemCount > threshold) {
    const toolError = ToolErrors.outputTooLarge(itemCount, threshold);

    return {
      shouldBlock: true,
      errorCode: toolError.errorCode,
      hints: [
        `Found ${itemCount} ${itemType}${itemCount === 1 ? '' : 's'} - exceeds safe limit of ${threshold}`,
        `Use charLength to paginate through results`,
        detailed
          ? 'Detailed results increase size - consider using charLength for pagination'
          : 'Consider using charLength to paginate large result sets',
      ],
    };
  }

  return { shouldBlock: false };
}
