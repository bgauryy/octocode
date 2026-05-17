/**
 * `lspRestart` tool execution (T2.2).
 *
 * Stops every pooled LSP client so the next request spawns a fresh
 * server. Recovers from confused server state (stale type graph,
 * incomplete project indexing, etc.) without restarting the whole
 * MCP process. Borrowed pattern from serena/Tritlo.
 *
 * @module tools/lsp_restart/execution
 */
import {
  releaseAllPooledClients,
  pooledClientCount,
} from '../../lsp/manager.js';

export interface LspRestartResult {
  status: 'hasResults' | 'empty' | 'error';
  clientsStopped: number;
  poolSizeAfter: number;
  error?: string;
  hints?: string[];
}

export async function executeLspRestart(): Promise<LspRestartResult> {
  const before = pooledClientCount();
  try {
    await releaseAllPooledClients();
  } catch (err) {
    return {
      status: 'error',
      clientsStopped: 0,
      poolSizeAfter: pooledClientCount(),
      error: err instanceof Error ? err.message : String(err),
      hints: [
        'Failed to stop one or more pooled LSP clients.',
        'You can usually safely retry — partially-stopped clients are tracked.',
      ],
    };
  }

  const after = pooledClientCount();
  if (before === 0) {
    return {
      status: 'empty',
      clientsStopped: 0,
      poolSizeAfter: after,
      hints: [
        'No pooled LSP clients were active. Restart was a no-op.',
        'Pool entries are created lazily on the first LSP-backed call.',
      ],
    };
  }

  return {
    status: 'hasResults',
    clientsStopped: before,
    poolSizeAfter: after,
    hints: [
      `Stopped ${before} pooled LSP client(s). The next LSP tool call will spawn a fresh server.`,
    ],
  };
}
