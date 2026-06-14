export function formatFileSize(bytes: number): string {
  if (bytes === 0) return '0.0B';
  if (bytes < 1024) return `${bytes}.0B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)}KB`;
  if (bytes < 1024 * 1024 * 1024)
    return `${(bytes / (1024 * 1024)).toFixed(1)}MB`;
  if (bytes < 1024 * 1024 * 1024 * 1024) {
    return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)}GB`;
  }
  return `${(bytes / (1024 * 1024 * 1024 * 1024)).toFixed(1)}TB`;
}

export function parseFileSize(sizeStr: string): number {
  const trimmed = sizeStr.trim();

  if (/^\d+$/.test(trimmed)) {
    return parseInt(trimmed, 10);
  }

  const decimalMatch = trimmed.match(/^(\d+(?:\.\d+)?)(B|KB|MB|GB|TB)$/i);
  if (decimalMatch && decimalMatch[1] && decimalMatch[2]) {
    const value = parseFloat(decimalMatch[1]);
    const unit = decimalMatch[2].toUpperCase();

    switch (unit) {
      case 'B':
        return Math.round(value);
      case 'KB':
        return Math.round(value * 1024);
      case 'MB':
        return Math.round(value * 1024 * 1024);
      case 'GB':
        return Math.round(value * 1024 * 1024 * 1024);
      case 'TB':
        return Math.round(value * 1024 * 1024 * 1024 * 1024);
    }
  }

  const match = trimmed.match(/^(\d+(?:\.\d+)?)([KMGT])$/i);
  if (!match || !match[1] || !match[2]) {
    throw new Error(`Invalid size format: ${sizeStr}`);
  }

  const value = parseFloat(match[1]);
  const unit = match[2].toUpperCase();

  switch (unit) {
    case 'K':
      return Math.round(value * 1024);
    case 'M':
      return Math.round(value * 1024 * 1024);
    case 'G':
      return Math.round(value * 1024 * 1024 * 1024);
    default:
      return Math.round(value * 1024 * 1024 * 1024 * 1024);
  }
}
