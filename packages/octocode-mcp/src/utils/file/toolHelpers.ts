/**
 * Helper utilities for local tools
 */

import path from 'path';
import { pathValidator } from 'octocode-security-utils/pathValidator';
import { ToolErrors } from '../../errors/errorFactories.js';
import type { BaseQueryLocal } from '@octocodeai/octocode-core';
import {
  createErrorResult,
  type UnifiedErrorResult,
} from '../response/error.js';

/**
 * Local error result type - compatible with UnifiedErrorResult
 */
type LocalErrorResult = UnifiedErrorResult;

export { createErrorResult };

/**
 * Path validation result with error result for tool returns
 */
interface ToolPathValidationResult {
  isValid: boolean;
  errorResult?: LocalErrorResult;
  sanitizedPath?: string;
}

/**
 * Generate hints for path-related errors based on the error type
 */
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

/**
 * Validate tool path and return validation result
 */
export function validateToolPath(
  query: BaseQueryLocal & { path: string },
  toolName: string
): ToolPathValidationResult {
  const cwd = process.cwd();
  const inputPath = query.path.replace(/^file:\/\//, '');
  const resolvedPath = path.resolve(inputPath);

  const validation = pathValidator.validate(inputPath);

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

/**
 * Options for checkLargeOutputSafety
 */
interface LargeOutputSafetyOptions {
  threshold?: number;
  itemType?: string;
  detailed?: boolean;
}

/**
 * Result of large output safety check
 */
interface LargeOutputSafetyResult {
  shouldBlock: boolean;
  errorCode?: string;
  hints?: string[];
}

/**
 * Check if output is too large and should be blocked
 */
export function checkLargeOutputSafety(
  itemCount: number,
  hasCharLength: boolean,
  options: LargeOutputSafetyOptions = {}
): LargeOutputSafetyResult {
  const { threshold = 100, itemType = 'item', detailed = false } = options;

  // If charLength is provided, pagination is already handled
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
