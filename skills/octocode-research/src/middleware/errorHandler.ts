import type { Request, Response, NextFunction } from 'express';
import type { z } from 'zod';
import { logError, logWarn, sanitizeQueryParams } from '../utils/logger.js';
import { logSessionError } from '../index.js';
import { fireAndForgetWithTimeout } from '../utils/asyncTimeout.js';

export interface ApiError extends Error {
  statusCode?: number;
  code?: string;
  details?: z.core.$ZodIssue[];
}

import { extractToolName } from '../utils/url.js';

export function errorHandler(
  error: ApiError,
  req: Request,
  res: Response,
  _next: NextFunction
): void {
  const statusCode = error.statusCode ?? 500;
  const isValidationError = statusCode === 400;

  if (isValidationError) {
    logWarn(`[VALIDATION] ${req.method} ${req.path}: ${error.message}`, {
      path: req.path,
      query: sanitizeQueryParams(req.query as Record<string, unknown>),
      details: error.details,
    });
  } else {
    logError(`[SERVER] ${req.method} ${req.path}: ${error.message}`, error);
  }

  const toolName = extractToolName(req.path);
  const errorCode = error.code ?? (isValidationError ? 'VALIDATION_ERROR' : 'INTERNAL_ERROR');
  fireAndForgetWithTimeout(
    () => logSessionError(toolName, errorCode),
    5000,
    'logSessionError'
  );

  const response: {
    success: false;
    error: {
      message: string;
      code: string;
      details?: z.core.$ZodIssue[];
    };
  } = {
    success: false,
    error: {
      message: error.message,
      code: error.code ?? 'INTERNAL_ERROR',
    },
  };

  if (isValidationError && error.details) {
    response.error.details = error.details;
  }

  res.status(statusCode).json(response);
}
