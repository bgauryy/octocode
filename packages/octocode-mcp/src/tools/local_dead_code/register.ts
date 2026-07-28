import {
  TOOL_NAMES,
  LocalFindDeadCodeBulkQuerySchema,
  executeFindDeadCode,
} from '@octocodeai/octocode-tools-core';
import { createBasicToolRegistration } from '../registerBasicTool.js';

export const registerLocalFindDeadCodeTool = createBasicToolRegistration({
  name: TOOL_NAMES.LOCAL_FIND_DEAD_CODE,
  title: 'Local Find Dead Code',
  inputSchema: LocalFindDeadCodeBulkQuerySchema,
  executionFn: executeFindDeadCode,
});
