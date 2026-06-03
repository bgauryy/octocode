/**
 * Route handler factory for reducing boilerplate in route files.
 *
 * Abstracts the common pattern:
 * 1. Parse and validate query params
 * 2. Execute tool with resilience wrapper
 * 3. Parse tool response
 * 4. Transform data to response format
 * 5. Send response with appropriate status
 *
 * @module utils/routeFactory
 */

import type { Request, Response, NextFunction, RequestHandler } from 'express';
import type { z } from 'zod';
import { parseAndValidate } from '../middleware/queryParser.js';
import { parseToolResponse, type ParsedResponse } from './responseParser.js';

/**
 * Resilience wrapper type - matches the signature of withLocalResilience, withGitHubResilience, etc.
 */
type ResilienceWrapper = <T>(
  fn: () => Promise<T>,
  toolName: string
) => Promise<T>;

/**
 * Transformer function type - converts parsed tool response to final response
 */
type ResponseTransformer<TQuery, TResponse> = (
  parsed: ParsedResponse,
  queries: TQuery[]
) => TResponse;

/**
 * MCP tool function type for the route factory.
 *
 * Uses `any` for the params intentionally: HTTP schemas produce slightly different
 * types than MCP tool functions expect (optional vs required fields, auto-generated id).
 * Type safety is enforced by Zod schema validation at runtime, not by static types
 * at this HTTP→MCP boundary.
 */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
type McpToolFn = (params: any) => Promise<any>;

/**
 * Route configuration options.
 * TQuery flows from schema validation through to response transformation.
 * The toolFn boundary uses McpToolFn because the schema's runtime transforms
 * (defaults, id generation) produce MCP-compatible data that TypeScript can't verify.
 */
interface RouteConfig<TQuery, TResponse> {
  /** Zod schema for query validation - accepts schemas with transforms */
  schema: z.ZodType<TQuery>;

  /** The MCP tool function to execute */
  toolFn: McpToolFn;

  /** Tool name for logging/resilience */
  toolName: string;

  /** Resilience wrapper (withLocalResilience, withGitHubResilience, etc.) */
  resilience: ResilienceWrapper;

  /** Transform parsed response to final format */
  transform: ResponseTransformer<TQuery, TResponse>;
}

/**
 * Create a route handler with standard error handling, validation, and resilience.
 *
 * @example
 * ```typescript
 * localRoutes.get('/search', createRouteHandler({
 *   schema: localSearchSchema,
 *   toolFn: localSearchCode,
 *   toolName: 'localSearchCode',
 *   resilience: withLocalResilience,
 *   transform: (parsed, queries) => {
 *     // Custom transformation logic
 *     return ResearchResponse.searchResults({ ... });
 *   },
 * }));
 * ```
 */
export function createRouteHandler<TQuery, TResponse>(
  config: RouteConfig<TQuery, TResponse>
): RequestHandler {
  const { schema, toolFn, toolName, resilience, transform } = config;

  return async (req: Request, res: Response, next: NextFunction): Promise<void> => {
    try {
      // 1. Parse and validate query params
      const queries = parseAndValidate(
        req.query as Record<string, unknown>,
        schema as z.ZodType<TQuery>
      ) as TQuery[];

      // 2. Execute tool with resilience wrapper
      const rawResult = await resilience(
        () => toolFn({ queries }),
        toolName
      );

      // 3. Parse tool response
      const parsed = parseToolResponse(rawResult as { content: Array<{ type: string; text: string }> });

      // 4. Transform to final response format
      const response = transform(parsed, queries);

      // 5. Send response with appropriate status
      res.status(parsed.isError ? 500 : 200).json(response);
    } catch (error) {
      next(error);
    }
  };
}
