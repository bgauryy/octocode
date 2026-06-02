import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import { toMCPSchema } from '../../types/toolTypes.js';
import { withResponseEnvelope } from '../../scheme/responseEnvelope.js';
import { TOOL_NAMES, DESCRIPTIONS } from '../toolMetadata/proxies.js';
import { BulkFindFilesSchema } from '../../scheme/localSchemaOverlay.js';
import { executeFindFiles } from './execution.js';
import { withBasicSecurityValidation } from '../../utils/securityBridge.js';
import { LocalFindFilesOutputSchema } from '@octocodeai/octocode-core/schemas/outputs';

// One-line schema disambiguation appended once at registration. The
// upstream description leaves the four name variants ambiguous; this note
// is read once when the tool list is fetched, never per-call.
const DESCRIPTION = `${DESCRIPTIONS[TOOL_NAMES.LOCAL_FIND_FILES]}
<nameVariants>
- name: case-sensitive glob ("*.ts")
- iname: case-insensitive glob ("README")
- names: multiple globs OR-combined
- pathPattern: glob over the full path (use for nested matches)
</nameVariants>`;

export function registerLocalFindFilesTool(server: McpServer) {
  return server.registerTool(
    TOOL_NAMES.LOCAL_FIND_FILES,
    {
      description: DESCRIPTION,
      inputSchema: toMCPSchema(BulkFindFilesSchema),
      outputSchema: toMCPSchema(
        withResponseEnvelope(LocalFindFilesOutputSchema)
      ),
      annotations: {
        title: 'Local Find Files',
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    withBasicSecurityValidation(executeFindFiles, TOOL_NAMES.LOCAL_FIND_FILES)
  );
}
