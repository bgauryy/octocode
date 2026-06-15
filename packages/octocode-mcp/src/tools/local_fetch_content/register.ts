import { z } from 'zod';
import {
  TOOL_NAMES,
  LocalFetchContentBulkQuerySchema,
  executeFetchContent,
  withResponseEnvelope,
} from '@octocodeai/octocode-tools-core';
import { LocalGetFileContentOutputSchema } from '@octocodeai/octocode-core/schemas/outputs';
import { createBasicToolRegistration } from '../registerBasicTool.js';

const MatchRangeSchema = z.object({ start: z.number(), end: z.number() });

const LocalGetFileContentFixedOutputSchema =
  LocalGetFileContentOutputSchema.extend({
    matchRanges: z
      .array(MatchRangeSchema)
      .optional()
      .describe(
        'Line ranges for each matched context block. Each entry has start (1-based first line) and end (1-based last line) of the block.'
      ),
    sourceChars: z
      .number()
      .optional()
      .describe(
        'Character length of the sanitized full source file before slicing, pagination, or symbols extraction.'
      ),
    sourceBytes: z
      .number()
      .optional()
      .describe(
        'UTF-8 byte length of the sanitized full source file before slicing, pagination, or symbols extraction.'
      ),
  });

export const registerLocalFetchContentTool = createBasicToolRegistration({
  name: TOOL_NAMES.LOCAL_FETCH_CONTENT,
  title: 'Local Fetch Content',
  inputSchema: LocalFetchContentBulkQuerySchema,
  outputSchema: withResponseEnvelope(LocalGetFileContentFixedOutputSchema),
  executionFn: executeFetchContent,
});
