import { describe, it, expect, vi, beforeEach } from 'vitest';
import type { Request, Response, NextFunction } from 'express';
import { errorHandler, type ApiError } from '../../middleware/errorHandler.js';
import type { z } from 'zod';
import { fireAndForgetWithTimeout } from '../../utils/asyncTimeout.js';

vi.mock('../../utils/logger.js', () => ({
  logError: vi.fn(),
  logWarn: vi.fn(),
  sanitizeQueryParams: vi.fn((q) => q),
}));

vi.mock('../../index.js', () => ({
  logSessionError: vi.fn().mockResolvedValue(undefined),
}));

vi.mock('../../utils/asyncTimeout.js', () => ({
  fireAndForgetWithTimeout: vi.fn(),
}));

describe('errorHandler', () => {
  let mockReq: Partial<Request>;
  let mockRes: Partial<Response>;
  let mockNext: NextFunction;
  let jsonMock: ReturnType<typeof vi.fn>;
  let statusMock: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    jsonMock = vi.fn();
    statusMock = vi.fn().mockReturnValue({ json: jsonMock });

    mockReq = {
      method: 'POST',
      path: '/tools/call/localSearchCode',
      query: {},
    };

    mockRes = {
      status: statusMock as unknown as Response['status'],
    };

    mockNext = vi.fn();
  });

  describe('status code handling', () => {
    it('uses error statusCode if provided', () => {
      const error: ApiError = new Error('Bad request');
      error.statusCode = 400;

      errorHandler(error, mockReq as Request, mockRes as Response, mockNext);

      expect(statusMock).toHaveBeenCalledWith(400);
    });

    it('defaults to 500 for server errors', () => {
      const error: ApiError = new Error('Internal error');

      errorHandler(error, mockReq as Request, mockRes as Response, mockNext);

      expect(statusMock).toHaveBeenCalledWith(500);
    });
  });

  describe('response format', () => {
    it('returns success: false', () => {
      const error: ApiError = new Error('Test error');

      errorHandler(error, mockReq as Request, mockRes as Response, mockNext);

      expect(jsonMock).toHaveBeenCalledWith(
        expect.objectContaining({
          success: false,
        })
      );
    });

    it('includes error message', () => {
      const error: ApiError = new Error('Test error message');

      errorHandler(error, mockReq as Request, mockRes as Response, mockNext);

      expect(jsonMock).toHaveBeenCalledWith(
        expect.objectContaining({
          error: expect.objectContaining({
            message: 'Test error message',
          }),
        })
      );
    });

    it('uses error code if provided', () => {
      const error: ApiError = new Error('Test error');
      error.code = 'CUSTOM_ERROR';

      errorHandler(error, mockReq as Request, mockRes as Response, mockNext);

      expect(jsonMock).toHaveBeenCalledWith(
        expect.objectContaining({
          error: expect.objectContaining({
            code: 'CUSTOM_ERROR',
          }),
        })
      );
    });

    it('defaults to INTERNAL_ERROR code', () => {
      const error: ApiError = new Error('Test error');

      errorHandler(error, mockReq as Request, mockRes as Response, mockNext);

      expect(jsonMock).toHaveBeenCalledWith(
        expect.objectContaining({
          error: expect.objectContaining({
            code: 'INTERNAL_ERROR',
          }),
        })
      );
    });
  });

  describe('validation error details', () => {
    it('includes details for 400 errors', () => {
      const error: ApiError = new Error('Validation failed');
      error.statusCode = 400;
      error.details = [
        { path: ['queries'], message: 'Required', code: 'invalid_type', expected: 'array' } as z.core.$ZodIssue,
      ];

      errorHandler(error, mockReq as Request, mockRes as Response, mockNext);

      expect(jsonMock).toHaveBeenCalledWith(
        expect.objectContaining({
          error: expect.objectContaining({
            details: expect.arrayContaining([
              expect.objectContaining({ path: ['queries'] }),
            ]),
          }),
        })
      );
    });

    it('excludes details for 500 errors', () => {
      const error: ApiError = new Error('Server error');
      error.details = [{ path: ['internal'], message: 'Debug info', code: 'custom' } as z.core.$ZodIssue];

      errorHandler(error, mockReq as Request, mockRes as Response, mockNext);

      const response = jsonMock.mock.calls[0][0];
      expect(response.error.details).toBeUndefined();
    });
  });

  describe('tool name extraction', () => {
    it('extracts tool name from /tools/call/:toolName path', () => {
      mockReq.path = '/tools/call/localSearchCode';
      const error: ApiError = new Error('Test');

      errorHandler(error, mockReq as Request, mockRes as Response, mockNext);

      expect(fireAndForgetWithTimeout).toHaveBeenCalled();
    });

    it('uses unknown for non-tool paths', () => {
      mockReq.path = '/prompts/list';
      const error: ApiError = new Error('Test');

      errorHandler(error, mockReq as Request, mockRes as Response, mockNext);

      expect(fireAndForgetWithTimeout).toHaveBeenCalled();
    });
  });
});
