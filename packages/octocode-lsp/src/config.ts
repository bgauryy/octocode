import { nativeBinding } from './native.js';
import type { LanguageServerConfig } from './types.js';

export function detectLanguageId(filePath: string): string {
  return nativeBinding.detectLanguageId(filePath) ?? 'plaintext';
}

export async function getLanguageServerForFile(
  filePath: string,
  workspaceRoot: string = process.cwd()
): Promise<LanguageServerConfig | null> {
  return (
    (nativeBinding.getLanguageServerForFile(filePath, workspaceRoot) as
      | LanguageServerConfig
      | undefined) ?? null
  );
}
