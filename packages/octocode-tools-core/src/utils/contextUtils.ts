import { createRequire } from 'node:module';

import type * as NativeContextUtils from '@octocodeai/octocode-context-utils';

export type {
  ExtractMatchingLinesOptions,
  ExtractMatchingLinesResult,
  FilterPatchOptions,
  FileSystemEntry,
  FileSystemQueryOptions,
  FileSystemQueryResult,
  MinifyResult,
  JsonInput,
  RipgrepParseOptions,
  RipgrepParseResult,
  SliceContentOptions,
  SliceContentResult,
  StructuralMatch,
  YamlConversionConfig,
} from '@octocodeai/octocode-context-utils';

type NativeContextUtilsModule = typeof NativeContextUtils;
type NativeLoader = () => NativeContextUtilsModule;

const require = createRequire(import.meta.url);
const NATIVE_PACKAGE_NAME = '@octocodeai/octocode-context-utils';

let cachedNative: NativeContextUtilsModule | undefined;
let nativeLoader: NativeLoader = () =>
  require(NATIVE_PACKAGE_NAME) as NativeContextUtilsModule;

export class ContextUtilsLoadError extends Error {
  constructor(readonly cause: unknown) {
    super(`Failed to load native dependency ${NATIVE_PACKAGE_NAME}`);
    this.name = 'ContextUtilsLoadError';
  }
}

function loadNative(): NativeContextUtilsModule {
  if (cachedNative) return cachedNative;

  try {
    cachedNative = nativeLoader();
    return cachedNative;
  } catch (error) {
    throw new ContextUtilsLoadError(error);
  }
}

export function setContextUtilsNativeLoaderForTesting(
  loader: NativeLoader
): void {
  nativeLoader = loader;
  cachedNative = undefined;
}

export function resetContextUtilsNativeLoaderForTesting(): void {
  nativeLoader = () => require(NATIVE_PACKAGE_NAME) as NativeContextUtilsModule;
  cachedNative = undefined;
}

export const contextUtils = {
  applyContentViewMinification(content: string, filePath: string): string {
    return loadNative().applyContentViewMinification(content, filePath);
  },

  applyMinification(content: string, filePath: string): string {
    return loadNative().applyMinification(content, filePath);
  },

  minifyContent(
    content: string,
    filePath: string
  ): Promise<NativeContextUtils.MinifyResult> {
    return loadNative().minifyContent(content, filePath);
  },

  minifyContentSync(content: string, filePath: string): string {
    return loadNative().minifyContentSync(content, filePath);
  },

  minifyContentResult(
    content: string,
    filePath: string
  ): NativeContextUtils.MinifyResult {
    return loadNative().minifyContentResult(content, filePath);
  },

  minifyMarkdownCore(content: string): string {
    return loadNative().minifyMarkdownCore(content);
  },

  extractSignatures(content: string, filePath: string): string | null {
    return loadNative().extractSignatures(content, filePath);
  },

  structuralSearch(
    content: string,
    filePath: string,
    pattern?: string | null,
    rule?: string | null
  ): NativeContextUtils.StructuralMatch[] {
    return loadNative().structuralSearch(content, filePath, pattern, rule);
  },

  getSemanticBoundaryOffsets(content: string, filePath: string): number[] {
    return loadNative().getSemanticBoundaryOffsets(content, filePath);
  },

  jsonToYamlString(
    jsonObject: NativeContextUtils.JsonInput,
    config?: NativeContextUtils.YamlConversionConfig | null
  ): string {
    return loadNative().jsonToYamlString(jsonObject, config);
  },

  parseRipgrepJson(
    stdout: string,
    options?: NativeContextUtils.RipgrepParseOptions | null
  ): NativeContextUtils.RipgrepParseResult {
    return loadNative().parseRipgrepJson(stdout, options);
  },

  queryFileSystem(
    options: NativeContextUtils.FileSystemQueryOptions
  ): NativeContextUtils.FileSystemQueryResult {
    return loadNative().queryFileSystem(options);
  },

  extractMatchingLines(
    content: string,
    pattern: string,
    options?: NativeContextUtils.ExtractMatchingLinesOptions | null
  ): NativeContextUtils.ExtractMatchingLinesResult {
    return loadNative().extractMatchingLines(content, pattern, options);
  },

  filterPatch(
    patch: string,
    options?: NativeContextUtils.FilterPatchOptions | null
  ): string {
    return loadNative().filterPatch(patch, options);
  },

  charToByteOffset(content: string, charIndex: number): number {
    return loadNative().charToByteOffset(content, charIndex);
  },

  byteToCharOffset(content: string, byteOffset: number): number {
    return loadNative().byteToCharOffset(content, byteOffset);
  },

  byteSliceContent(
    content: string,
    byteStart: number,
    byteEnd: number
  ): string {
    return loadNative().byteSliceContent(content, byteStart, byteEnd);
  },

  sliceContent(
    content: string,
    charOffset: number,
    charLength: number,
    options?: NativeContextUtils.SliceContentOptions | null
  ): NativeContextUtils.SliceContentResult {
    return loadNative().sliceContent(content, charOffset, charLength, options);
  },

  get SIGNATURES_ONLY_HINT(): string {
    return loadNative().SIGNATURES_ONLY_HINT;
  },
};
