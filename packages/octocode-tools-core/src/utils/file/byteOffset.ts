import {
  contextUtils,
  type SliceContentOptions,
  type SliceContentResult,
} from '../contextUtils.js';

export function byteSlice(
  content: string,
  byteStart: number,
  byteEnd: number
): string {
  return contextUtils.byteSliceContent(content, byteStart, byteEnd);
}

export function byteToCharIndex(content: string, byteOffset: number): number {
  return contextUtils.byteToCharOffset(content, byteOffset);
}

export function charToByteIndex(content: string, charIndex: number): number {
  return contextUtils.charToByteOffset(content, charIndex);
}

export function getByteLength(content: string): number {
  return contextUtils.charToByteOffset(content, content.length);
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

export function sliceContent(
  content: string,
  charOffset: number,
  charLength: number,
  options?: SliceContentOptions
): SliceContentResult {
  return contextUtils.sliceContent(content, charOffset, charLength, options);
}

export type { SliceContentOptions, SliceContentResult };
