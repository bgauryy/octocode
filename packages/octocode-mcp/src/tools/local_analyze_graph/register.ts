import {
  TOOL_NAMES,
  LocalAnalyzeGraphBulkQuerySchema,
  executeAnalyzeGraph,
} from '@octocodeai/octocode-tools-core';
import { createBasicToolRegistration } from '../registerBasicTool.js';

export const registerLocalAnalyzeGraphTool = createBasicToolRegistration({
  name: TOOL_NAMES.LOCAL_ANALYZE_GRAPH,
  title: 'Local Analyze Graph',
  inputSchema: LocalAnalyzeGraphBulkQuerySchema,
  executionFn: executeAnalyzeGraph,
});
