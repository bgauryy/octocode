import {
  TOOL_NAMES,
  LocalViewStructureBulkQuerySchema,
  LocalViewStructureOutputSchema,
  executeViewStructure,
} from '@octocodeai/octocode-tools-core';
import { createBasicToolRegistration } from '../registerBasicTool.js';

export const registerLocalViewStructureTool = createBasicToolRegistration({
  name: TOOL_NAMES.LOCAL_VIEW_STRUCTURE,
  title: 'Local View Structure',
  inputSchema: LocalViewStructureBulkQuerySchema,
  outputSchema: LocalViewStructureOutputSchema,
  executionFn: executeViewStructure,
});
