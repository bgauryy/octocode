import type { CompleteMetadata } from 'octocode-mcp/public';
import { initialize, loadToolContent } from 'octocode-mcp/public';

let mcpContent: CompleteMetadata | null = null;
let initPromise: Promise<CompleteMetadata> | null = null;


export async function initializeMcpContent(): Promise<CompleteMetadata> {
  if (mcpContent) return mcpContent;

  if (initPromise) return initPromise;

  initPromise = (async () => {
    await initialize();
    const content = await loadToolContent();
    mcpContent = content;
    return content;
  })();

  return initPromise;
}


export function getMcpContent(): CompleteMetadata {
  if (!mcpContent) {
    throw new Error('mcpContent not initialized. Call initializeMcpContent() at server startup.');
  }
  return mcpContent;
}


export function isMcpInitialized(): boolean {
  return mcpContent !== null;
}
