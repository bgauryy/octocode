import { nativeBinding } from './native.js';

export async function safeReadFile(filePath: string): Promise<string | null> {
  try {
    return nativeBinding.safeReadFile(filePath);
  } catch {
    return null;
  }
}

export async function safeReadLineWindow(
  filePath: string,
  lineZeroBased: number,
  contextLines: number
): Promise<string | null> {
  try {
    return nativeBinding.safeReadLineWindow(
      filePath,
      lineZeroBased,
      contextLines
    );
  } catch {
    return null;
  }
}

export function validateLSPServerPath(
  command: string,
  _workspaceRoot?: string
): { isValid: boolean; resolvedPath?: string; error?: string } {
  try {
    return {
      isValid: true,
      resolvedPath: nativeBinding.validateLspServerPath(command),
    };
  } catch (error) {
    return {
      isValid: false,
      error: error instanceof Error ? error.message : String(error),
    };
  }
}
