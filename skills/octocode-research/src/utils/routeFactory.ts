import type { Request, Response, NextFunction, RequestHandler } from 'express';
import type { z } from 'zod';
import { parseAndValidate } from '../middleware/queryParser.js';
import { parseToolResponse, type ParsedResponse } from './responseParser.js';


type ResilienceWrapper = <T>(
  fn: () => Promise<T>,
  toolName: string
) => Promise<T>;


type ResponseTransformer<TQuery, TResponse> = (
  parsed: ParsedResponse,
  queries: TQuery[]
) => TResponse;


// eslint-disable-next-line @typescript-eslint/no-explicit-any
type McpToolFn = (params: any) => Promise<any>;


interface RouteConfig<TQuery, TResponse> {
  
  schema: z.ZodType<TQuery>;

  
  toolFn: McpToolFn;

  
  toolName: string;

  
  resilience: ResilienceWrapper;

  
  transform: ResponseTransformer<TQuery, TResponse>;
}

export function createRouteHandler<TQuery, TResponse>(
  config: RouteConfig<TQuery, TResponse>
): RequestHandler {
  const { schema, toolFn, toolName, resilience, transform } = config;

  return async (req: Request, res: Response, next: NextFunction): Promise<void> => {
    try {
      const queries = parseAndValidate(
        req.query as Record<string, unknown>,
        schema as z.ZodType<TQuery>
      ) as TQuery[];

      const rawResult = await resilience(
        () => toolFn({ queries }),
        toolName
      );

      const parsed = parseToolResponse(rawResult as { content: Array<{ type: string; text: string }> });

      const response = transform(parsed, queries);

      res.status(parsed.isError ? 500 : 200).json(response);
    } catch (error) {
      next(error);
    }
  };
}
