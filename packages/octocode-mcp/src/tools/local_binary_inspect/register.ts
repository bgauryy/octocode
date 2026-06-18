import {
  TOOL_NAMES,
  LocalBinaryInspectBulkQuerySchema,
  executeInspectBinary,
} from '@octocodeai/octocode-tools-core';
import { createBasicToolRegistration } from '../registerBasicTool.js';

export const registerLocalBinaryInspectTool = createBasicToolRegistration({
  name: TOOL_NAMES.LOCAL_BINARY_INSPECT,
  title: 'Local Binary Inspect',
  inputSchema: LocalBinaryInspectBulkQuerySchema,
  executionFn: executeInspectBinary,
});
