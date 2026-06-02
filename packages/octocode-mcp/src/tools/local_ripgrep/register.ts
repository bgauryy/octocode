import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import { toMCPSchema } from '../../types/toolTypes.js';
import { withResponseEnvelope } from '../../scheme/responseEnvelope.js';
import { TOOL_NAMES, DESCRIPTIONS } from '../toolMetadata/proxies.js';
import { BulkRipgrepQuerySchema } from '../../scheme/localSchemaOverlay.js';
import { executeRipgrepSearch } from './execution.js';
import { withBasicSecurityValidation } from '../../utils/securityBridge.js';
import { LocalSearchCodeOutputSchema } from '@octocodeai/octocode-core/schemas/outputs';

// One-line disambiguation appended once at server registration. Not a hint —
// the agent reads it once when the tool surface is described, never again.
const DESCRIPTION = `${DESCRIPTIONS[TOOL_NAMES.LOCAL_RIPGREP]}
<workspace>relative paths resolve against the server's working directory; absolute paths must be within an allowed root (home directory or ALLOWED_PATHS).</workspace>
<vsLocal>use localFindFiles for name/time/size lookups; ripgrep is for content matches.</vsLocal>`;

export function registerLocalRipgrepTool(server: McpServer) {
  return server.registerTool(
    TOOL_NAMES.LOCAL_RIPGREP,
    {
      description: DESCRIPTION,
      inputSchema: toMCPSchema(BulkRipgrepQuerySchema),
      outputSchema: toMCPSchema(
        withResponseEnvelope(LocalSearchCodeOutputSchema)
      ),
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
