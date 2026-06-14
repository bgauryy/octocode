export function byteSlice(
  content: string,
  byteStart: number,
  byteEnd: number
): string {
  const buffer = Buffer.from(content, 'utf8');
  return buffer.slice(byteStart, byteEnd).toString('utf8');
}

export function byteToCharIndex(content: string, byteOffset: number): number {
  if (byteOffset === 0) return 0;

  const buffer = Buffer.from(content, 'utf8');
  const clampedOffset = Math.min(byteOffset, buffer.length);
  const substring = buffer.slice(0, clampedOffset).toString('utf8');
  return substring.length;
}

export function charToByteIndex(content: string, charIndex: number): number {
  return Buffer.byteLength(content.substring(0, charIndex), 'utf8');
}

export function getByteLength(content: string): number {
  return Buffer.byteLength(content, 'utf8');
}

export function convertByteMatchToChar(
  content: string,
  byteOffset: number,
  byteLength: number
): {
  charOffset: number;
  charLength: number;
  text: string;
} {
  const text = byteSlice(content, byteOffset, byteOffset + byteLength);
  const charOffset = byteToCharIndex(content, byteOffset);

  return {
    charOffset,
    charLength: text.length,
    text,
  };
}
