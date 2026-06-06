import { BaseCommandBuilder } from './BaseCommandBuilder.js';
import type { z } from 'zod';
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
    if (this.isWindows) {
      throw new Error(
        'Windows is not supported for localFindFiles. Use localViewStructure or localSearchCode instead.'
      );
    }

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
      this.addArg('-o');
    }

    this.addFilters(query);
    this.addArg('-print0');

    return this;
  }

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
        this.addOption('-regextype', query.regexType);
      }
      const normalizedRegex = this.normalizeRegexForFullPath(query.regex);
      this.addOption('-regex', normalizedRegex);
    }

    if (query.empty) {
      this.addFlag('-empty');
    }

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
        this.addOption('-perm', '+111');
      }
    }

    if (query.readable) {
      if (this.isLinux) {
        this.addFlag('-readable');
      } else {
        this.addOption('-perm', '+444');
      }
    }

    if (query.writable) {
      if (this.isLinux) {
        this.addFlag('-writable');
      } else {
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

  private normalizeSizeForPlatform(size: string): string {
    const match = size.match(/^(\d+)([ckKMGmg])?$/);
    if (!match || !match[1]) {
      return size;
    }

    const value = parseInt(match[1], 10);
    const suffix = match[2];

    if (!suffix) {
      return size;
    }

    const upperSuffix = suffix.toUpperCase();

    switch (upperSuffix) {
      case 'C':
        return `${value}c`;
      case 'K':
        return `${value}k`;
      case 'M':
        if (this.isMacOS) {
          return `${value * 1024 * 1024}c`;
        }
        return `${value}M`;
      case 'G':
        if (this.isMacOS) {
          return `${value * 1024 * 1024 * 1024}c`;
        }
        return `${value}G`;
      default:
        return size;
    }
  }
}
