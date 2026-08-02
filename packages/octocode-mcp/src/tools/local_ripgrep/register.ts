import {
  TOOL_NAMES,
  LocalRipgrepBulkQuerySchema,
  executeRipgrepSearch,
} from '@octocodeai/octocode-tools-core';
import { createBasicToolRegistration } from '../registerBasicTool.js';

export const registerLocalRipgrepTool = createBasicToolRegistration({
  name: TOOL_NAMES.LOCAL_RIPGREP,
  title: 'Local Ripgrep Search',
  inputSchema: LocalRipgrepBulkQuerySchema,
  executionFn: executeRipgrepSearch,
});
