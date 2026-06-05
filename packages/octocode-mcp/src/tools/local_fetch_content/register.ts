import { z } from 'zod';
import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import { toMCPSchema } from '../../types/toolTypes.js';
import { withResponseEnvelope } from '../../scheme/responseEnvelope.js';
import { TOOL_NAMES, DESCRIPTIONS } from '../toolMetadata/proxies.js';
import { BulkFetchContentQuerySchema } from '../../scheme/localSchemaOverlay.js';
import { executeFetchContent } from './execution.js';
import { withBasicSecurityValidation } from '../../utils/securityBridge.js';
import { LocalGetFileContentOutputSchema } from '@octocodeai/octocode-core/schemas/outputs';

/**
 * matchRanges emitted at runtime: Array<{ start: number; end: number }>.
 * The upstream core schema uses z.array(z.array(z.number())) which is wrong.
 * Override here so the MCP outputSchema accurately reflects the actual shape.
 */
const MatchRangeSchema = z.object({ start: z.number(), end: z.number() });

const LocalGetFileContentFixedOutputSchema = LocalGetFileContentOutputSchema.extend({
  matchRanges: z.array(MatchRangeSchema).optional().describe(
    'Line ranges for each matched context block. Each entry has start (1-based first line) and end (1-based last line) of the block.'
  ),
});

/**
 * Register the local fetch content tool with the MCP server.
 */
export function registerLocalFetchContentTool(server: McpServer) {
  return server.registerTool(
    TOOL_NAMES.LOCAL_FETCH_CONTENT,
    {
      description: DESCRIPTIONS[TOOL_NAMES.LOCAL_FETCH_CONTENT],
      inputSchema: toMCPSchema(BulkFetchContentQuerySchema),
      outputSchema: toMCPSchema(
        withResponseEnvelope(LocalGetFileContentFixedOutputSchema)
      ),
      annotations: {
        title: 'Local Fetch Content',
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    withBasicSecurityValidation(
      executeFetchContent,
      TOOL_NAMES.LOCAL_FETCH_CONTENT
    )
  );
}
