/** Redact common credential shapes before compaction text reaches extension artifacts. */
export function redactCompactionText(text: string): string {
  return text
    .replace(/Bearer\s+[A-Za-z0-9._~+/-]{12,}={0,2}/gi, 'Bearer [REDACTED]')
    .replace(/\b(?:sk-[A-Za-z0-9_-]{12,}|ghp_[A-Za-z0-9]{12,}|github_pat_[A-Za-z0-9_]{12,}|xox[baprs]-[A-Za-z0-9-]{12,})\b/g, '[REDACTED]')
    .replace(
      /(["']?(?:authorization|cookie|set-cookie|token|secret|password|api[_-]?key|access[_-]?key|credential)["']?\s*[:=]\s*)(["']?)([^\s,;}\]]+)\2/gi,
      '$1$2[REDACTED]$2',
    );
}
