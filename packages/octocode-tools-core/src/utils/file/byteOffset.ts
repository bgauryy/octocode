import {
  charToByteOffset,
  byteToCharOffset,
  byteSliceContent,
  sliceContent,
  type SliceContentOptions,
  type SliceContentResult,
} from '@octocodeai/octocode-context-utils';

export function byteSlice(
  content: string,
  byteStart: number,
  byteEnd: number
): string {
  return byteSliceContent(content, byteStart, byteEnd);
}

export function byteToCharIndex(content: string, byteOffset: number): number {
  return byteToCharOffset(content, byteOffset);
}

export function charToByteIndex(content: string, charIndex: number): number {
  return charToByteOffset(content, charIndex);
}

export function getByteLength(content: string): number {
  return charToByteOffset(content, content.length);
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
  return { charOffset, charLength: text.length, text };
}

export { sliceContent, type SliceContentOptions, type SliceContentResult };
