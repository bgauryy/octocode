import { BaseCommandBuilder } from './BaseCommandBuilder.js';
import type { z } from 'zod/v4';
import type { FindFilesQuerySchema } from '@octocodeai/octocode-core/schemas';

type FindFilesQuery = z.infer<typeof FindFilesQuerySchema> & {
  modifiedAfter?: string;
};

export class FindCommandBuilder extends BaseCommandBuilder {
  private isMacOS: boolean;
  private isLinux: boolean;
  private isWindows: boolean;

  constructor() {
    super('find');
    this.isMacOS = process.platform === 'darwin';
    this.isLinux = process.platform === 'linux';
    this.isWindows = process.platform === 'win32';
  }

  fromQuery(
    query: Partial<FindFilesQuery> & Pick<FindFilesQuery, 'path'>
  ): this {
    // Windows is not supported - find command doesn't exist
    if (this.isWindows) {
      throw new Error(
        'Windows is not supported for localFindFiles. Use localViewStructure or localSearchCode instead.'
      );
    }

    // macOS requires -E flag BEFORE path for extended regex
    if (this.isMacOS && query.regex) {
      this.addFlag('-E');
    }

    this.addArg(query.path);

    if (this.isLinux) {
      this.addFlag('-O3');
    }

    if (query.maxDepth !== undefined) {
      this.addOption('-maxdepth', query.maxDepth);
    }

    if (query.minDepth !== undefined) {
      this.addOption('-mindepth', query.minDepth);
    }

    const hasExcludeDir = query.excludeDir && query.excludeDir.length > 0;
    if (hasExcludeDir) {
      this.buildExcludeDirPrune(query.excludeDir!);
      this.addArg('-o'); // Connect prune to filters with OR
    }

    this.addFilters(query);
    this.addArg('-print0');

    return this;
  }

  /**
   * Builds the excludeDir prune block:
   * ( -path "*\/dir" -o -path "*\/dir/*" ) -prune
   * Repeated for each directory
   */
  private buildExcludeDirPrune(excludeDirs: string[]): void {
    this.addArg('(');
    excludeDirs.forEach((dir, index) => {
      if (index > 0) {
        this.addArg('-o');
      }
      this.addArg('-path');
      this.addArg(`*/${dir}`);
      this.addArg('-o');
      this.addArg('-path');
      this.addArg(`*/${dir}/*`);
    });
    this.addArg(')');
    this.addArg('-prune');
  }

  /**
   * Adds all filter options (type, names, size, time, etc.)
   * These must come AFTER the prune block when excludeDir is used
   */
  private addFilters(query: Partial<FindFilesQuery>): void {
    if (query.type) {
      this.addOption('-type', query.type);
    }

    if (query.names && query.names.length > 0) {
      if (query.names.length === 1) {
        this.addOption('-name', query.names[0]!);
      } else {
        this.addArg('(');
        query.names.forEach((name: string, index: number) => {
          if (index > 0) {
            this.addArg('-o');
          }
          this.addOption('-name', name);
        });
        this.addArg(')');
      }
    } else if (query.name) {
      this.addOption('-name', query.name);
    }

    if (query.iname) {
      this.addOption('-iname', query.iname);
    }

    if (query.pathPattern) {
      this.addOption('-path', query.pathPattern);
    }

    if (query.regex) {
      if (this.isLinux && query.regexType) {
        // GNU find uses -regextype
        this.addOption('-regextype', query.regexType);
      }
      // macOS -E flag was added at the beginning
      // find -regex matches against the FULL path (e.g. /Users/.../foo.test.ts),
      // not just the filename. Prepend .* so filename-oriented patterns work as expected.
      const normalizedRegex = this.normalizeRegexForFullPath(query.regex);
      this.addOption('-regex', normalizedRegex);
    }

    if (query.empty) {
      this.addFlag('-empty');
    }

    // Size filters - normalize suffix for cross-platform compatibility
    // BUG FIX: macOS BSD find only accepts lowercase 'k' for kilobytes
    // Linux GNU find accepts both 'K' and 'k', plus 'M', 'G' etc.
    // Before: sizeGreater="10K" → find -size +10K → FAILS on macOS
    // After:  sizeGreater="10K" → find -size +10k → WORKS on all platforms
    if (query.sizeGreater) {
      this.addOption(
        '-size',
        `+${this.normalizeSizeForPlatform(query.sizeGreater)}`
      );
    }

    if (query.sizeLess) {
      this.addOption(
        '-size',
        `-${this.normalizeSizeForPlatform(query.sizeLess)}`
      );
    }

    if (query.modifiedWithin) {
      const parsed = this.parseTimeString(query.modifiedWithin);
      if (parsed) this.addOption(parsed.unit, `-${parsed.value}`);
    }

    if (query.modifiedBefore) {
      const parsed = this.parseTimeString(query.modifiedBefore);
      if (parsed) this.addOption(parsed.unit, `+${parsed.value}`);
    }

    if (query.modifiedAfter) {
      const parsed = this.parseTimeString(query.modifiedAfter);
      if (parsed) this.addOption(parsed.unit, `-${parsed.value}`);
    }

    if (query.accessedWithin) {
      const parsed = this.parseTimeStringAccess(query.accessedWithin);
      if (parsed) this.addOption(parsed.unit, `-${parsed.value}`);
    }

    if (query.permissions) {
      this.addOption('-perm', query.permissions);
    }

    if (query.executable) {
      if (this.isLinux) {
        this.addFlag('-executable');
      } else {
        // macOS: use -perm +111 (any execute bit)
        this.addOption('-perm', '+111');
      }
    }

    if (query.readable) {
      if (this.isLinux) {
        this.addFlag('-readable');
      } else {
        // macOS: use -perm +444 (any read bit)
        this.addOption('-perm', '+444');
      }
    }

    if (query.writable) {
      if (this.isLinux) {
        this.addFlag('-writable');
      } else {
        // macOS: use -perm +222 (any write bit)
        this.addOption('-perm', '+222');
      }
    }
  }

  simple(path: string, name: string): this {
    this.addArg(path);
    this.addOption('-name', name);
    return this;
  }

  type(type: 'f' | 'd' | 'l'): this {
    this.addOption('-type', type);
    return this;
  }

  name(pattern: string): this {
    this.addOption('-name', pattern);
    return this;
  }

  iname(pattern: string): this {
    this.addOption('-iname', pattern);
    return this;
  }

  maxDepth(depth: number): this {
    this.addOption('-maxdepth', depth);
    return this;
  }

  minDepth(depth: number): this {
    this.addOption('-mindepth', depth);
    return this;
  }

  size(size: string): this {
    this.addOption('-size', size);
    return this;
  }

  mtime(time: string): this {
    this.addOption('-mtime', time);
    return this;
  }

  path(path: string): this {
    this.addArg(path);
    return this;
  }

  /**
   * Ensures regex patterns match against the full path.
   *
   * `find -regex` matches against the ENTIRE path (e.g. /Users/.../foo.test.ts),
   * not just the filename. Users commonly provide filename-oriented patterns like
   * `\.(test|spec)\.ts$` which silently return 0 results.
   *
   * This method prepends `.*` when the pattern doesn't already account for the
   * full path, so `\.(test|spec)\.ts$` becomes `.*\.(test|spec)\.ts$` and works.
   *
   * Patterns that already start with `.*`, `/`, or `^` are left unchanged.
   */
  private normalizeRegexForFullPath(regex: string): string {
    if (
      regex.startsWith('.*') ||
      regex.startsWith('/') ||
      regex.startsWith('^')
    ) {
      return regex;
    }
    return `.*${regex}`;
  }

  /**
   * Parses a relative time string (e.g. "7d", "2h", "1w", "3m") and returns
   * the appropriate find flag + value.  Returns `null` for unrecognised
   * formats (e.g. ISO timestamps) so callers can skip or warn rather than
   * silently emitting `-mtime -0` or `-mtime +0`.
   */
  private parseTimeString(timeStr: string): {
    value: number;
    unit: '-mtime' | '-mmin';
  } | null {
    const match = timeStr.match(/^(\d+)([hdwm])$/);
    if (!match || !match[1] || !match[2]) {
      return null;
    }

    const value = parseInt(match[1], 10);
    const timeUnit = match[2];

    switch (timeUnit) {
      case 'h':
        // Use -mmin for hours (converted to minutes)
        return { value: value * 60, unit: '-mmin' };
      case 'd':
        return { value, unit: '-mtime' };
      case 'w':
        return { value: value * 7, unit: '-mtime' };
      case 'm':
        return { value: value * 30, unit: '-mtime' };
      default:
        return { value, unit: '-mtime' };
    }
  }

  /**
   * Parses access time string (similar to mtime but uses -atime/-amin).
   * Returns null for unrecognised formats so callers can skip gracefully.
   */
  private parseTimeStringAccess(timeStr: string): {
    value: number;
    unit: '-atime' | '-amin';
  } | null {
    const match = timeStr.match(/^(\d+)([hdwm])$/);
    if (!match || !match[1] || !match[2]) {
      return null;
    }

    const value = parseInt(match[1], 10);
    const timeUnit = match[2];

    switch (timeUnit) {
      case 'h':
        // Use -amin for hours (converted to minutes)
        return { value: value * 60, unit: '-amin' };
      case 'd':
        return { value, unit: '-atime' };
      case 'w':
        return { value: value * 7, unit: '-atime' };
      case 'm':
        return { value: value * 30, unit: '-atime' };
      default:
        return { value, unit: '-atime' };
    }
  }

  /**
   * Normalizes size suffix for cross-platform compatibility.
   *
   * PLATFORM DIFFERENCES:
   * - macOS BSD find: Only supports 'c' (bytes) and 'k' (kilobytes) - LOWERCASE ONLY
   * - Linux GNU find: Supports 'c', 'w', 'b', 'k', 'K', 'M', 'G' (case-sensitive)
   *
   * BUG FIX HISTORY:
   * Before: Users passing "10K" would get "find: -size: +10K: illegal trailing character" on macOS
   * After:  "10K" is normalized to "10k", works on both macOS and Linux
   *
   * EXAMPLES:
   * - "10K" → "10k" (kilobytes, normalized for macOS)
   * - "10k" → "10k" (already lowercase)
   * - "100" → "100" (bytes, no suffix)
   * - "1M" → "1048576c" (megabytes converted to bytes for macOS compatibility)
   * - "1G" → "1073741824c" (gigabytes converted to bytes for macOS compatibility)
   *
   * @param size - Size string like "10k", "10K", "1M", "1G", or raw number
   * @returns Normalized size string compatible with both macOS and Linux
   */
  private normalizeSizeForPlatform(size: string): string {
    // Match number followed by optional suffix
    const match = size.match(/^(\d+)([ckKMGmg])?$/);
    if (!match || !match[1]) {
      return size; // Return as-is if invalid format
    }

    const value = parseInt(match[1], 10);
    const suffix = match[2];

    if (!suffix) {
      // No suffix means bytes (works on all platforms)
      return size;
    }

    const upperSuffix = suffix.toUpperCase();

    switch (upperSuffix) {
      case 'C':
        // Bytes - use lowercase for consistency
        return `${value}c`;
      case 'K':
        // Kilobytes - macOS only accepts lowercase 'k'
        return `${value}k`;
      case 'M':
        // Megabytes - macOS doesn't support 'M', convert to bytes
        if (this.isMacOS) {
          return `${value * 1024 * 1024}c`;
        }
        return `${value}M`;
      case 'G':
        // Gigabytes - macOS doesn't support 'G', convert to bytes
        if (this.isMacOS) {
          return `${value * 1024 * 1024 * 1024}c`;
        }
        return `${value}G`;
      default:
        return size;
    }
  }
}
