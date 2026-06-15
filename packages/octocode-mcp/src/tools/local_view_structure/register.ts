import {
  TOOL_NAMES,
  LocalViewStructureBulkQuerySchema,
  executeViewStructure,
  withResponseEnvelope,
} from '@octocodeai/octocode-tools-core';
import { LocalViewStructureOutputSchema } from '@octocodeai/octocode-core/schemas/outputs';
import { createBasicToolRegistration } from '../registerBasicTool.js';

export const registerLocalViewStructureTool = createBasicToolRegistration({
  name: TOOL_NAMES.LOCAL_VIEW_STRUCTURE,
  title: 'Local View Structure',
  inputSchema: LocalViewStructureBulkQuerySchema,
  outputSchema: withResponseEnvelope(LocalViewStructureOutputSchema),
  executionFn: executeViewStructure,
});
