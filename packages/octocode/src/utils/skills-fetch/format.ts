export function formatSkillName(name: string): string {
  const acronyms = ['PR', 'API', 'UI', 'CLI', 'MCP', 'AI'];

  return name
    .replace(/\.md$/i, '')
    .replace(/[-_]/g, ' ')
    .replace(/\b\w/g, c => c.toUpperCase())
    .replace(new RegExp(`\\b(${acronyms.join('|')})\\b`, 'gi'), match =>
      match.toUpperCase()
    );
}

export function extractFirstParagraph(content: string): string | null {
  const withoutFrontmatter = content.replace(/^---[\s\S]*?---\s*/, '');

  const lines = withoutFrontmatter.split('\n');
  let paragraph = '';

  for (const line of lines) {
    const trimmed = line.trim();
    if (!trimmed) {
      if (paragraph) break;
      continue;
    }
    if (trimmed.startsWith('#')) continue;
    paragraph += (paragraph ? ' ' : '') + trimmed;
  }

  return paragraph ? paragraph.slice(0, 200) : null;
}
