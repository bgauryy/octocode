import path from 'path';
import { pathValidator } from '@octocodeai/octocode-engine/pathValidator';
import { ToolErrors } from '../../errors/errorFactories.js';
import type { BaseQueryLocal } from '@octocodeai/octocode-core/extra-types';
type PartialBaseQueryLocal = Partial<BaseQueryLocal>;
import {
  createErrorResult,
  type UnifiedErrorResult,
} from '../response/error.js';
import { getConfigSync } from '@octocodeai/config';

type LocalErrorResult = UnifiedErrorResult;

export { createErrorResult };

type ToolPathValidationResult =
  | { isValid: false; errorResult: LocalErrorResult; sanitizedPath?: undefined }
  | { isValid: true; sanitizedPath: string; errorResult?: undefined };

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

    return {
      isValid: false,
      errorResult: createErrorResult(toolError, query, {
        toolName,
        extra: {
          cwd,
          resolvedPath,
        },
      }),
    };
  }

  return {
    isValid: true,
    sanitizedPath: validation.sanitizedPath ?? resolvedPath,
  };
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
