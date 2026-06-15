import {
  TOOL_NAMES,
  LocalFindFilesBulkQuerySchema,
  executeFindFiles,
  withResponseEnvelope,
} from '@octocodeai/octocode-tools-core';
import { LocalFindFilesOutputSchema } from '@octocodeai/octocode-core/schemas/outputs';
import { createBasicToolRegistration } from '../registerBasicTool.js';

export const registerLocalFindFilesTool = createBasicToolRegistration({
  name: TOOL_NAMES.LOCAL_FIND_FILES,
  title: 'Local Find Files',
  inputSchema: LocalFindFilesBulkQuerySchema,
  outputSchema: withResponseEnvelope(LocalFindFilesOutputSchema),
  executionFn: executeFindFiles,
});
