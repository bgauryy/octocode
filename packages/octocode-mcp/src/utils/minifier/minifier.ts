import { getExtension } from '../file/filters.js';
import type { FileTypeMinifyConfig, MinifyResult } from './minifierTypes.js';
import { MINIFY_CONFIG, INDENTATION_SENSITIVE_NAMES } from './minifierTypes.js';
import {
  minifyConservativeCore,
  minifyAggressiveCore,
  minifyJsonCore,
  minifyGeneralCore,
  minifyMarkdownCore,
  minifyCSSCore,
  minifyHTMLCore,
  minifyJavaScriptCore,
  minifyWithTerser,
  minifyCSSAsync,
  minifyHTMLAsync,
} from './minifierStrategies.js';

const MINIFIER_EXT_OPTIONS = { lowercase: true, fallback: 'txt' } as const;

function getFileConfig(filePath: string): FileTypeMinifyConfig {
  const ext = getExtension(filePath, MINIFIER_EXT_OPTIONS);
  const baseName = (filePath.split('/').pop() || '').toLowerCase();

  if (INDENTATION_SENSITIVE_NAMES.has(baseName)) {
    return { strategy: 'conservative', comments: 'hash' };
  }

  return MINIFY_CONFIG.fileTypes[ext] || { strategy: 'general' };
}

export function minifyContentSync(content: string, filePath: string): string {
  const config = getFileConfig(filePath);
  const ext = getExtension(filePath, MINIFIER_EXT_OPTIONS);

  try {
    switch (config.strategy) {
      case 'terser':
        return minifyJavaScriptCore(content);

      case 'json':
        return minifyJsonCore(content).content;

      case 'conservative':
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
        const result = await minifyWithTerser(content);
        return {
          content: result.content,
          failed: result.failed,
          type: result.failed ? 'failed' : 'terser',
          ...(result.reason && { reason: result.reason }),
        };
      }

      case 'json': {
        const result = minifyJsonCore(content);
        return {
          content: result.content,
          failed: result.failed,
          type: result.failed ? 'failed' : 'json',
          ...(result.reason && { reason: result.reason }),
        };
      }

      case 'conservative':
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
