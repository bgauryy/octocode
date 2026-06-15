import {
  BulkLspGetSemanticContentQuerySchema,
  LspGetSemanticContentOutputSchema,
  executeLspGetSemanticContent,
  LSP_GET_SEMANTIC_CONTENT_TOOL_NAME,
  withResponseEnvelope,
} from '@octocodeai/octocode-tools-core';
import { createBasicToolRegistration } from '../../registerBasicTool.js';

export const registerLspGetSemanticContentTool = createBasicToolRegistration({
  name: LSP_GET_SEMANTIC_CONTENT_TOOL_NAME,
  title: 'Get Semantic Content',
  inputSchema: BulkLspGetSemanticContentQuerySchema,
  outputSchema: withResponseEnvelope(LspGetSemanticContentOutputSchema),
  executionFn: executeLspGetSemanticContent,
});
