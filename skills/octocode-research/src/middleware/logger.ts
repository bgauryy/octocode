import type { Request, Response, NextFunction } from 'express';
import { randomUUID } from 'crypto';
import { logToolCall, sanitizeQueryParams } from '../utils/logger.js';
import { resultLog, errorLog } from '../utils/colors.js';
import { extractToolName } from '../utils/url.js';


function getRequestId(req: Request): string {
  const existingId = req.headers['x-request-id'];
  if (typeof existingId === 'string' && existingId.length > 0) {
    return existingId;
  }
  return randomUUID();
}

export function requestLogger(
  req: Request,
  res: Response,
  next: NextFunction
): void {
  const start = Date.now();
  const requestId = getRequestId(req);

  res.setHeader('x-request-id', requestId);

  res.on('finish', () => {
    const duration = Date.now() - start;
    const status = res.statusCode;
    const statusIcon = status >= 400 ? '❌' : '✅';
    const success = status < 400;

    const resultMessage = `${statusIcon} ${req.method} ${req.path} ${status} ${duration}ms`;

    if (success) {
      console.log(resultLog(resultMessage));
    } else {
      console.log(errorLog(resultMessage));
    }

    if (req.path !== '/health') {
      logToolCall({
        tool: extractToolName(req.path),
        route: req.path,
        method: req.method,
        params: sanitizeQueryParams(req.query as Record<string, unknown>),
        duration,
        success,
        error: success ? undefined : `HTTP ${status}`,
        requestId,
      });
    }
  });

  next();
}

