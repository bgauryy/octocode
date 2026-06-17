import {
  GITHUB_HISTORY_TOOL_NAME,
  HistoryBulkQuerySchema,
  getMultipleHistories,
} from '@octocodeai/octocode-tools-core';
import { createRemoteToolRegistration } from '../registerRemoteTool.js';

export const registerGitHubHistoryTool = createRemoteToolRegistration({
  name: GITHUB_HISTORY_TOOL_NAME,
  title: 'GitHub History',
  inputSchema: HistoryBulkQuerySchema,
  executionFn: getMultipleHistories,
});
