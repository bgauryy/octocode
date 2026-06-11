import { getExtension } from '../utils/fileExtension.js';
import type { FileTypeMinifyConfig, MinifyResult } from '../types/index.js';
import { MINIFY_CONFIG, INDENTATION_SENSITIVE_NAMES } from '../types/index.js';
import {
  minifyConservativeCore,
  minifyAggressiveCore,
  minifyJsonCore,
  minifyGeneralCore,
  minifyMarkdownCore,
  minifyCSSCore,
  minifyHTMLCore,
  minifyJavaScriptCore,
  minifyTypeScriptLikeSync,
  minifyTypeScriptLike,
  minifyWithTerserSync,
  minifyWithTerser,
  minifyCSSAsync,
  minifyHTMLAsync,
  minifyComponentAsync,
} from './strategies.js';

const MINIFIER_EXT_OPTIONS = { lowercase: true, fallback: 'txt' } as const;
const TYPESCRIPT_LIKE_EXTENSIONS = new Set(['ts', 'tsx', 'jsx']);
const COMPONENT_EXTENSIONS = new Set(['vue', 'svelte']);

function getFileConfig(filePath: string): FileTypeMinifyConfig {
  const ext = getExtension(filePath, MINIFIER_EXT_OPTIONS);
  const baseName = (filePath.split(/[\\/]/).pop() || '').toLowerCase();

  if (INDENTATION_SENSITIVE_NAMES.has(baseName)) {
    return { strategy: 'conservative', comments: 'hash' };
  }

  return MINIFY_CONFIG.fileTypes[ext] || { strategy: 'general' };
}

function hasFlowSyntaxHint(content: string): boolean {
  return (
    content.includes('@flow') ||
    content.includes('import type ') ||
    content.includes('export type ') ||
    content.includes(' as any as ')
  );
}

function useTerserFallbackIfSupported(
  ext: string,
  content: string,
  fallbackContent: string
): string | null {
  if (ext !== 'jsx' && !(ext === 'js' && hasFlowSyntaxHint(content))) {
    return null;
  }

  return fallbackContent !== content && fallbackContent.length < content.length
    ? fallbackContent
    : null;
}

function shorterThanOriginal(
  content: string,
  candidate: string
): string | null {
  return candidate.length < content.length ? candidate : null;
}

function minifyTypeScriptLikeWithFallbackSync(
  content: string,
  filePath: string,
  fallbackContent: string
): string {
  const result = minifyTypeScriptLikeSync(content, filePath);
  const candidate = !result.failed
    ? shorterThanOriginal(content, result.content)
    : null;

  return candidate ?? fallbackContent;
}

async function minifyTypeScriptLikeWithFallback(
  content: string,
  filePath: string,
  fallbackContent: string
): Promise<{ content: string; reason?: string }> {
  const result = await minifyTypeScriptLike(content, filePath);
  const candidate = !result.failed
    ? shorterThanOriginal(content, result.content)
    : null;

  return {
    content: candidate ?? fallbackContent,
    ...(result.reason && { reason: result.reason }),
  };
}

export function minifyContentSync(content: string, filePath: string): string {
  const config = getFileConfig(filePath);
  const ext = getExtension(filePath, MINIFIER_EXT_OPTIONS);

  try {
    switch (config.strategy) {
      case 'terser':
        if (TYPESCRIPT_LIKE_EXTENSIONS.has(ext)) {
          return minifyTypeScriptLikeWithFallbackSync(
            content,
            filePath,
            minifyJavaScriptCore(content)
          );
        }
        return minifyWithTerserSync(content).content;

      case 'json':
        return minifyJsonCore(content).content;

      case 'conservative':
        if (TYPESCRIPT_LIKE_EXTENSIONS.has(ext)) {
          return minifyTypeScriptLikeWithFallbackSync(
            content,
            filePath,
            minifyConservativeCore(content, config)
          );
        }
        return minifyConservativeCore(content, config);

      case 'markdown':
        return minifyMarkdownCore(content);

      case 'aggressive':
        if (['css', 'less', 'scss'].includes(ext)) {
          return minifyCSSCore(content);
        }
        if (['html', 'htm', 'xml', 'svg'].includes(ext)) {
          return minifyHTMLCore(content);
        }
        return minifyAggressiveCore(content, config);

      case 'general':
      default:
        return minifyGeneralCore(content);
    }
  } /* v8 ignore start */ catch {
    return content;
  } /* v8 ignore stop */
}

export async function minifyContent(
  content: string,
  filePath: string
): Promise<MinifyResult> {
  try {
    const MAX_SIZE = 1024 * 1024;
    const contentSize = Buffer.byteLength(content, 'utf8');

    if (contentSize > MAX_SIZE) {
      return {
        content,
        failed: true,
        type: 'failed',
        reason: `File too large: ${(contentSize / 1024 / 1024).toFixed(2)}MB exceeds 1MB limit`,
      };
    }

    const config = getFileConfig(filePath);
    const ext = getExtension(filePath, MINIFIER_EXT_OPTIONS);

    switch (config.strategy) {
      case 'terser': {
        if (TYPESCRIPT_LIKE_EXTENSIONS.has(ext)) {
          const fallback = minifyJavaScriptCore(content);
          const result = await minifyTypeScriptLikeWithFallback(
            content,
            filePath,
            fallback
          );

          return {
            content: result.content,
            failed: false,
            type: 'terser',
            ...(result.reason && { reason: result.reason }),
          };
        }

        const result = await minifyWithTerser(content);
        if (result.failed) {
          const fallbackContent = useTerserFallbackIfSupported(
            ext,
            content,
            minifyJavaScriptCore(content)
          );
          if (fallbackContent !== null) {
            return {
              content: fallbackContent,
              failed: false,
              type: 'terser',
              ...(result.reason && { reason: result.reason }),
            };
          }
        }

        return {
          content: result.content,
          failed: result.failed,
          type: result.failed ? 'failed' : 'terser',
          ...(result.reason && { reason: result.reason }),
        };
      }

      case 'json': {
        const result = minifyJsonCore(content);
        // minifyJsonCore always returns failed:false and never sets reason;
        // the fields are kept explicit for future-proofing.
        return {
          content: result.content,
          failed: false,
          type: 'json',
        };
      }

      case 'conservative':
        if (TYPESCRIPT_LIKE_EXTENSIONS.has(ext)) {
          const fallback = minifyConservativeCore(content, config);
          const result = await minifyTypeScriptLikeWithFallback(
            content,
            filePath,
            fallback
          );

          return {
            content: result.content,
            failed: false,
            type: 'conservative',
            ...(result.reason && { reason: result.reason }),
          };
        }

        return {
          content: minifyConservativeCore(content, config),
          failed: false,
          type: 'conservative',
        };

      case 'general':
        return {
          content: minifyGeneralCore(content),
          failed: false,
          type: 'general',
        };

      case 'markdown':
        return {
          content: minifyMarkdownCore(content),
          failed: false,
          type: 'markdown',
        };

      case 'aggressive': {
        if (['css', 'less', 'scss'].includes(ext)) {
          const result = await minifyCSSAsync(content);
          return {
            content: result.content,
            failed: false,
            type: 'aggressive',
            ...(result.reason && { reason: result.reason }),
          };
        }

        if (['html', 'htm'].includes(ext)) {
          const result = await minifyHTMLAsync(content);
          return {
            content: result.content,
            failed: false,
            type: 'aggressive',
            ...(result.reason && { reason: result.reason }),
          };
        }

        if (COMPONENT_EXTENSIONS.has(ext)) {
          const result = await minifyComponentAsync(content);
          return {
            content: result.content,
            failed: false,
            type: 'aggressive',
            ...(result.reason && { reason: result.reason }),
          };
        }

        return {
          content: minifyAggressiveCore(content, config),
          failed: false,
          type: 'aggressive',
        };
      }
    }
  } catch (error: unknown) {
    return {
      content,
      failed: true,
      type: 'failed',
      reason: `Unexpected minification error: ${error instanceof Error ? error.message : 'Unknown error'}`,
    };
  }
}

export { MINIFY_CONFIG };
