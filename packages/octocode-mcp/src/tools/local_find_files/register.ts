import {
  TOOL_NAMES,
  LocalFindFilesBulkQuerySchema,
  executeFindFiles,
} from '@octocodeai/octocode-tools-core';
import { createBasicToolRegistration } from '../registerBasicTool.js';

export const registerLocalFindFilesTool = createBasicToolRegistration({
  name: TOOL_NAMES.LOCAL_FIND_FILES,
  title: 'Local Find Files',
  inputSchema: LocalFindFilesBulkQuerySchema,
  executionFn: executeFindFiles,
});
