import {
  BulkLspGetSemanticsQuerySchema,
  executeLspGetSemantics,
  LSP_GET_SEMANTIC_CONTENT_TOOL_NAME,
} from '@octocodeai/octocode-tools-core';
import { createBasicToolRegistration } from '../../registerBasicTool.js';

export const registerLspGetSemanticsTool = createBasicToolRegistration({
  name: LSP_GET_SEMANTIC_CONTENT_TOOL_NAME,
  title: 'Get Semantic Content',
  inputSchema: BulkLspGetSemanticsQuerySchema,
  executionFn: executeLspGetSemantics,
});
