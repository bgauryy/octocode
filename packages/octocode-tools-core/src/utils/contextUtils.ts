import { createRequire } from 'node:module';

import type * as NativeContextUtils from '@octocodeai/octocode-engine';

type NativeContextUtilsModule = typeof NativeContextUtils;
type NativeLoader = () => NativeContextUtilsModule;

const require = createRequire(import.meta.url);
const NATIVE_PACKAGE_NAME = '@octocodeai/octocode-engine';

// Embedded single-file builds (Node SEA) pre-load the engine addon and publish
// it on globalThis; the require-by-package-name path below cannot resolve once
// this file is inlined into a bundle with no node_modules on disk.
const defaultNativeLoader: NativeLoader = () =>
  ((globalThis as { __OCTOCODE_ENGINE_BINDING__?: NativeContextUtilsModule })
    .__OCTOCODE_ENGINE_BINDING__ ??
    require(NATIVE_PACKAGE_NAME)) as NativeContextUtilsModule;

let cachedNative: NativeContextUtilsModule | undefined;
let nativeLoader: NativeLoader = defaultNativeLoader;

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
  nativeLoader = defaultNativeLoader;
  cachedNative = undefined;
}

export const contextUtils = {
  applyContentViewMinification(content: string, filePath: string): string {
    return loadNative().applyContentViewMinification(content, filePath);
  },

  minifyContent(
    content: string,
    filePath: string
  ): Promise<NativeContextUtils.MinifyResult> {
    return loadNative().minifyContent(content, filePath);
  },

  extractSignatures(content: string, filePath: string): string | null {
    return loadNative().extractSignatures(content, filePath);
  },

  /**
   * Native JS/TS document symbols as a JSON `DocumentSymbol[]` string, or null
   * when oxc declines the input (non-JS/TS, oversized, hard parse failure, or
   * no symbols). Server-free, syntax-only — no type inference.
   */
  extractJsSymbols(content: string, filePath: string): string | null {
    return loadNative().extractJsSymbols(content, filePath);
  },

  /**
   * Native JS/TS graph facts as a JSON `GraphFacts` object. Syntax-level only:
   * declarations/imports/exports, containment, and direct call expressions.
   */
  extractGraphFacts(content: string, filePath: string): string | null {
    return loadNative().extractGraphFacts(content, filePath);
  },

  /** Walk, read, and extract graph facts on the native worker pool in one call. */
  scanGraphFacts(
    options: NativeContextUtils.GraphFactsScanOptions
  ): Promise<NativeContextUtils.GraphFactsScanResult> {
    return loadNative().scanGraphFacts(options);
  },

  /**
   * Canonical lowercase extensions (no leading dot) the native oxc JS/TS path
   * handles. Source of truth lives in the engine — gate native dispatch on this.
   */
  getSupportedJsTsExtensions(): string[] {
    return loadNative().getSupportedJsTsExtensions();
  },

  getGrammarCapabilities(): NativeContextUtils.GrammarCapability[] {
    return loadNative().getGrammarCapabilities();
  },

  structuralSearch(
    content: string,
    filePath: string,
    pattern?: string | null,
    rule?: string | null
  ): Promise<NativeContextUtils.StructuralMatch[]> {
    return loadNative().structuralSearch(content, filePath, pattern, rule);
  },

  structuralSearchFiles(
    options: NativeContextUtils.StructuralSearchFilesOptions
  ): Promise<NativeContextUtils.StructuralSearchFilesResult> {
    return loadNative().structuralSearchFiles(options);
  },

  inspectSyntaxTree(
    content: string,
    filePath: string,
    options?: NativeContextUtils.SyntaxTreeInspectOptions
  ): Promise<NativeContextUtils.SyntaxTreeInspectResult> {
    return loadNative().inspectSyntaxTree(content, filePath, options);
  },

  validateRipgrepPattern(
    pattern: string,
    fixedString?: boolean | null,
    perlRegex?: boolean | null
  ): NativeContextUtils.RipgrepPatternValidationResult {
    return loadNative().validateRipgrepPattern(pattern, fixedString, perlRegex);
  },

  getSemanticBoundaryOffsets(
    content: string,
    filePath: string
  ): Promise<number[]> {
    return loadNative().getSemanticBoundaryOffsets(content, filePath);
  },

  jsonToYamlString(
    jsonObject: NativeContextUtils.JsonInput,
    config?: NativeContextUtils.YamlConversionConfig | null
  ): string {
    return loadNative().jsonToYamlString(jsonObject, config);
  },

  searchRipgrep(
    options: NativeContextUtils.RipgrepSearchOptions
  ): Promise<NativeContextUtils.RipgrepParseResult> {
    return loadNative().searchRipgrep(options);
  },

  queryFileSystem(
    options: NativeContextUtils.FileSystemQueryOptions
  ): Promise<NativeContextUtils.FileSystemQueryResult> {
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
};
