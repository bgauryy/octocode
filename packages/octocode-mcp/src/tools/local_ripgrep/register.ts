import {
  TOOL_NAMES,
  LocalRipgrepBulkQuerySchema,
  executeRipgrepSearch,
  withResponseEnvelope,
} from '@octocodeai/octocode-tools-core';
import { LocalSearchCodeOutputSchema } from '@octocodeai/octocode-core/schemas/outputs';
import { createBasicToolRegistration } from '../registerBasicTool.js';

export const registerLocalRipgrepTool = createBasicToolRegistration({
  name: TOOL_NAMES.LOCAL_RIPGREP,
  title: 'Local Ripgrep Search',
  inputSchema: LocalRipgrepBulkQuerySchema,
  outputSchema: withResponseEnvelope(LocalSearchCodeOutputSchema),
  executionFn: executeRipgrepSearch,
});
