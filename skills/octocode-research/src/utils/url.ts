export function extractToolName(path: string): string {
  const toolCallMatch = path.match(/^\/tools\/call\/(\w+)$/);
  if (toolCallMatch) {
    return toolCallMatch[1];
  }

  const parts = path.split('/').filter(Boolean);
  if (parts.length >= 2) {
    if (parts[0] === 'tools' && parts[1] === 'call' && parts[2]) {
        return parts[2];
    }
    return parts[0] + parts[1].charAt(0).toUpperCase() + parts[1].slice(1);
  }
  return parts.join('/') || 'unknown';
}
