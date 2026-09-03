/**
 * Escape untrusted metadata before interpolating it into XML-like system-prompt
 * addenda. The model still sees the literal text, but it cannot close or forge
 * harness-owned delimiter blocks.
 */
export function escapePromptMetadata(text: string): string {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;');
}
