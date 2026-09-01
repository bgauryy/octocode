// Relative-window grammar shared with local files time filters: <n><unit>
// where unit ∈ h(ours) d(ays) w(eeks) m(onths) y(ears). The GitHub commits API
// only accepts absolute ISO-8601, so a raw "30d" was silently dropped (returned
// an empty list = false-absence). Convert relative → ISO here; pass absolute
// dates through; warn (never silently skip) on an unparseable value.
const RELATIVE_WINDOW_RE = /^(\d+)\s*([hdwmy])$/i;

export function resolveDateWindow(value: string): {
  value?: string;
  warning?: string;
} {
  const trimmed = value.trim();
  const match = RELATIVE_WINDOW_RE.exec(trimmed);
  if (match) {
    const n = Number(match[1]);
    const unit = match[2]!.toLowerCase();
    const d = new Date();
    switch (unit) {
      case 'h':
        d.setUTCHours(d.getUTCHours() - n);
        break;
      case 'd':
        d.setUTCDate(d.getUTCDate() - n);
        break;
      case 'w':
        d.setUTCDate(d.getUTCDate() - n * 7);
        break;
      case 'm':
        d.setUTCMonth(d.getUTCMonth() - n);
        break;
      case 'y':
        d.setUTCFullYear(d.getUTCFullYear() - n);
        break;
    }
    return { value: d.toISOString() };
  }
  if (Number.isNaN(Date.parse(trimmed))) {
    return {
      warning: `"${value}" is not a valid date or relative window — use e.g. "30d", "2w", "6m", "1y", or an ISO date like "2026-01-01". Filter skipped.`,
    };
  }
  return { value: trimmed };
}
