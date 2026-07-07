import {
  TOOL_NAMES,
  LocalFetchContentBulkQuerySchema,
  LocalGetFileContentOutputSchema,
  executeFetchContent,
} from '@octocodeai/octocode-tools-core';
import { createBasicToolRegistration } from '../registerBasicTool.js';

export const registerLocalFetchContentTool = createBasicToolRegistration({
  name: TOOL_NAMES.LOCAL_FETCH_CONTENT,
  title: 'Local Fetch Content',
  inputSchema: LocalFetchContentBulkQuerySchema,
  outputSchema: LocalGetFileContentOutputSchema,
  executionFn: executeFetchContent,
});
