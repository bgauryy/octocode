import { createRequire } from 'node:module';

import type * as NativeContextUtils from '@octocodeai/octocode-engine';

export type {
  ExtractMatchingLinesOptions,
  ExtractMatchingLinesResult,
  FilterPatchOptions,
  FileSystemEntry,
  FileSystemQueryOptions,
  FileSystemQueryResult,
  GraphFactCapability,
  GraphFactCall,
  GraphFactDeclaration,
  GraphFactEdge,
  GraphFactExport,
  GraphFactImport,
  GraphFacts,
  LineDiffOp,
  MinifyResult,
  JsonInput,
  RipgrepParseOptions,
  RipgrepParseResult,
  RipgrepSearchOptions,
  SliceContentOptions,
  SliceContentResult,
  StructuralMatch,
  YamlConversionConfig,
} from '@octocodeai/octocode-engine';

type NativeContextUtilsModule = typeof NativeContextUtils;
type NativeLoader = () => NativeContextUtilsModule;

const require = createRequire(import.meta.url);
const NATIVE_PACKAGE_NAME = '@octocodeai/octocode-engine';

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

  /**
   * Native JS/TS document symbols as a JSON `DocumentSymbol[]` string, or null
   * when oxc declines the input (non-JS/TS, oversized, hard parse failure, or
   * no symbols). Server-free, syntax-only — no type inference.
   */
  extractJsSymbols(content: string, filePath: string): string | null {
    return loadNative().extractJsSymbols(content, filePath);
  },

  /**
   * Native in-file references as a JSON `Range[]` (declaration first), or null
   * when oxc declines the input or the cursor is not on a resolvable binding.
   * Same-file only, syntax-only — no type inference, no cross-file resolution.
   */
  findInFileReferences(
    content: string,
    filePath: string,
    line: number,
    character: number
  ): string | null {
    return loadNative().findInFileReferences(
      content,
      filePath,
      line,
      character
    );
  },

  /**
   * Native JS/TS graph facts as a JSON `GraphFacts` object. Syntax-level only:
   * declarations/imports/exports, containment, and direct call expressions.
   */
  extractGraphFacts(content: string, filePath: string): string | null {
    return loadNative().extractGraphFacts(content, filePath);
  },

  /**
   * Canonical lowercase extensions (no leading dot) the native oxc JS/TS path
   * handles. Source of truth lives in the engine — gate native dispatch on this.
   */
  getSupportedJsTsExtensions(): string[] {
    return loadNative().getSupportedJsTsExtensions();
  },

  /**
   * Canonical lowercase extensions (no leading dot) that can emit native
   * GraphFacts. JS/TS use OXC; other entries use tree-sitter syntax inventory.
   */
  getSupportedGraphFactExtensions(): string[] {
    return loadNative().getSupportedGraphFactExtensions();
  },

  /**
   * Native graph-fact capability matrix as a JSON `GraphFactCapability[]`.
   */
  getGraphFactCapabilities(): string {
    return loadNative().getGraphFactCapabilities();
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

  getSupportedStructuralExtensions(): string[] {
    return loadNative().getSupportedStructuralExtensions();
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

  parseRipgrepJson(
    stdout: string,
    options?: NativeContextUtils.RipgrepParseOptions | null
  ): NativeContextUtils.RipgrepParseResult {
    return loadNative().parseRipgrepJson(stdout, options);
  },

  searchRipgrep(
    options: NativeContextUtils.RipgrepSearchOptions
  ): Promise<NativeContextUtils.RipgrepParseResult> {
    return loadNative().searchRipgrep(options);
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

  computeLineDiff(
    oldText: string,
    newText: string
  ): NativeContextUtils.LineDiffOp[] {
    return loadNative().computeLineDiff(oldText, newText);
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
