import { minifyContentSync } from './minifier.js';

export function applyMinification(content: string, filePath: string): string {
  try {
    const minifiedContent = minifyContentSync(content, filePath);
    return minifiedContent.length < content.length ? minifiedContent : content;
  } catch {
    return content;
  }
}
