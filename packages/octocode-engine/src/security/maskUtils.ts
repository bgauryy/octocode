export type Span = { start: number; end: number };

export function maskEveryOtherChar(text: string): string {
  let result = '';
  for (let i = 0; i < text.length; i++) {
    result += i % 2 === 0 ? '*' : text[i];
  }
  return result;
}

export function deduplicateSpans(spans: Span[]): Span[] {
  spans.sort((a, b) => a.start - b.start);
  const result: Span[] = [];
  let lastEnd = 0;
  for (const span of spans) {
    if (span.start >= lastEnd) {
      result.push(span);
      lastEnd = span.end;
    }
  }
  return result;
}

export function applyMaskToSpans(text: string, spans: Span[]): string {
  let result = '';
  let position = 0;
  for (const span of spans) {
    result += text.slice(position, span.start);
    result += maskEveryOtherChar(text.slice(span.start, span.end));
    position = span.end;
  }
  return result + text.slice(position);
}
