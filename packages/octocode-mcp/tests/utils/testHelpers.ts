import type { ContentBlock } from '@modelcontextprotocol/sdk/types.js';

function isTextContent(
  content: ContentBlock | undefined
): content is { type: 'text'; text: string } {
  return content?.type === 'text';
}

export function getTextContent(content: ContentBlock[]): string {
  const firstContent = content[0];
  if (!isTextContent(firstContent)) {
    throw new Error(
      `Expected text content, got ${firstContent?.type || 'undefined'}`
    );
  }
  return firstContent.text;
}
