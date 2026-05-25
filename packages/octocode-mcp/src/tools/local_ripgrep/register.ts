import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import { toMCPSchema } from '../../types/toolTypes.js';
import { withTsvEnvelope } from '../../scheme/tsvEnvelope.js';
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import { LOCAL_RIPGREP_DESCRIPTION } from '@octocodeai/octocode-core';
import { BulkRipgrepQuerySchema } from '../../scheme/localSchemaOverlay.js';
import { executeRipgrepSearch } from './execution.js';
import { withBasicSecurityValidation } from '../../utils/securityBridge.js';
import { LocalSearchCodeOutputSchema } from '@octocodeai/octocode-core';

// One-line disambiguation appended once at server registration. Not a hint —
// the agent reads it once when the tool surface is described, never again.
const DESCRIPTION = `${LOCAL_RIPGREP_DESCRIPTION}
<vsLocal>use localFindFiles for name/time/size lookups; ripgrep is for content matches.</vsLocal>`;

export function registerLocalRipgrepTool(server: McpServer) {
  return server.registerTool(
    TOOL_NAMES.LOCAL_RIPGREP,
    {
      description: DESCRIPTION,
      inputSchema: toMCPSchema(BulkRipgrepQuerySchema),
      outputSchema: toMCPSchema(withTsvEnvelope(LocalSearchCodeOutputSchema)),
      annotations: {
        title: 'Local Ripgrep Search',
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    withBasicSecurityValidation(executeRipgrepSearch, TOOL_NAMES.LOCAL_RIPGREP)
  );
}
