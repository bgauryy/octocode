import { describe, it, expect } from 'vitest';
import {
  localSearchSchema,
  localContentSchema,
  localFindSchema,
  localStructureSchema,
  lspDefinitionSchema,
  lspReferencesSchema,
  lspCallsSchema,
  githubSearchSchema,
  githubContentSchema,
  githubReposSchema,
  githubStructureSchema,
  githubPRsSchema,
  packageSearchSchema,
} from '../../validation/schemas.js';


describe('localSearchSchema', () => {
  it('parses minimal valid input', () => {
    const result = localSearchSchema.safeParse({
      pattern: 'useState',
      path: '/project/src',
    });
    expect(result.success).toBe(true);
    if (result.success) {
      expect(result.data.pattern).toBe('useState');
      expect(result.data.path).toBe('/project/src');
    }
  });

  it('converts numeric strings to numbers', () => {
    const result = localSearchSchema.safeParse({
      pattern: 'test',
      path: '/project',
      contextLines: '5',
      maxFiles: '10',
      limit: '100',
    });
    expect(result.success).toBe(true);
    if (result.success) {
      expect(result.data.contextLines).toBe(5);
      expect(result.data.maxFiles).toBe(10);
      expect(result.data.limit).toBe(100);
    }
  });

  it('converts boolean strings', () => {
    const result = localSearchSchema.safeParse({
      pattern: 'test',
      path: '/project',
      filesOnly: 'true',
      caseInsensitive: 'false',
      hidden: 'true',
    });
    expect(result.success).toBe(true);
    if (result.success) {
      expect(result.data.filesOnly).toBe(true);
      expect(result.data.caseInsensitive).toBe(false);
      expect(result.data.hidden).toBe(true);
    }
  });

  it('maps deprecated context alias to contextLines', () => {
    const result = localSearchSchema.safeParse({
      pattern: 'test',
      path: '/project',
      context: '3',
    });
    expect(result.success).toBe(true);
    if (result.success) {
      expect(result.data.contextLines).toBe(3);
    }
  });

  it('maps deprecated maxResults alias to limit', () => {
    const result = localSearchSchema.safeParse({
      pattern: 'test',
      path: '/project',
      maxResults: '50',
    });
    expect(result.success).toBe(true);
    if (result.success) {
      expect(result.data.limit).toBe(50);
    }
  });

  it('injects research defaults', () => {
    const result = localSearchSchema.safeParse({
      pattern: 'test',
      path: '/project',
    });
    expect(result.success).toBe(true);
    if (result.success) {
      expect(result.data.mainResearchGoal).toBe('HTTP API request');
      expect(result.data.researchGoal).toBe('Execute tool via HTTP');
      expect(result.data.reasoning).toBe('HTTP API call');
      expect(result.data.id).toMatch(/^http-\d+$/);
    }
  });

  it('preserves custom research context', () => {
    const result = localSearchSchema.safeParse({
      pattern: 'test',
      path: '/project',
      mainResearchGoal: 'Custom goal',
      researchGoal: 'Custom research',
      reasoning: 'Custom reason',
    });
    expect(result.success).toBe(true);
    if (result.success) {
      expect(result.data.mainResearchGoal).toBe('Custom goal');
      expect(result.data.researchGoal).toBe('Custom research');
      expect(result.data.reasoning).toBe('Custom reason');
    }
  });

  it('rejects empty pattern', () => {
    const result = localSearchSchema.safeParse({
      pattern: '',
      path: '/project',
    });
    expect(result.success).toBe(false);
  });

  it('rejects path traversal', () => {
    const result = localSearchSchema.safeParse({
      pattern: 'test',
      path: '../../../etc/passwd',
    });
    expect(result.success).toBe(false);
  });

  it('converts comma-separated include to array', () => {
    const result = localSearchSchema.safeParse({
      pattern: 'test',
      path: '/project',
      include: 'ts,js,tsx',
    });
    expect(result.success).toBe(true);
    if (result.success) {
      expect(result.data.include).toEqual(['ts', 'js', 'tsx']);
    }
  });
});

describe('localContentSchema', () => {
  it('parses minimal valid input', () => {
    const result = localContentSchema.safeParse({ path: '/project/file.ts' });
    expect(result.success).toBe(true);
  });

  it('converts line numbers from strings', () => {
    const result = localContentSchema.safeParse({
      path: '/project/file.ts',
      startLine: '10',
      endLine: '50',
    });
    expect(result.success).toBe(true);
    if (result.success) {
      expect(result.data.startLine).toBe(10);
      expect(result.data.endLine).toBe(50);
    }
  });

  it('converts fullContent boolean', () => {
    const result = localContentSchema.safeParse({
      path: '/project/file.ts',
      fullContent: 'true',
    });
    expect(result.success).toBe(true);
    if (result.success) {
      expect(result.data.fullContent).toBe(true);
    }
  });
});

describe('localFindSchema', () => {
  it('parses minimal valid input', () => {
    const result = localFindSchema.safeParse({ path: '/project' });
    expect(result.success).toBe(true);
  });

  it('maps deprecated pattern to name', () => {
    const result = localFindSchema.safeParse({
      path: '/project',
      pattern: '*.ts',
    });
    expect(result.success).toBe(true);
    if (result.success) {
      expect(result.data.name).toBe('*.ts');
    }
  });

  it('transforms file type codes', () => {
    const result = localFindSchema.safeParse({
      path: '/project',
      type: 'file',
    });
    expect(result.success).toBe(true);
    if (result.success) {
      expect(result.data.type).toBe('f');
    }
  });

  it('passes through short type codes', () => {
    const result = localFindSchema.safeParse({
      path: '/project',
      type: 'd',
    });
    expect(result.success).toBe(true);
    if (result.success) {
      expect(result.data.type).toBe('d');
    }
  });
});

describe('localStructureSchema', () => {
  it('parses with depth as string', () => {
    const result = localStructureSchema.safeParse({
      path: '/project',
      depth: '3',
    });
    expect(result.success).toBe(true);
    if (result.success) {
      expect(result.data.depth).toBe(3);
    }
  });

  it('maps showHidden alias to hidden', () => {
    const result = localStructureSchema.safeParse({
      path: '/project',
      showHidden: 'true',
    });
    expect(result.success).toBe(true);
    if (result.success) {
      expect(result.data.hidden).toBe(true);
    }
  });
});


describe('lspDefinitionSchema', () => {
  it('parses valid input with string numbers', () => {
    const result = lspDefinitionSchema.safeParse({
      uri: '/project/src/index.ts',
      symbolName: 'MyClass',
      lineHint: '42',
    });
    expect(result.success).toBe(true);
    if (result.success) {
      expect(result.data.lineHint).toBe(42);
      expect(result.data.contextLines).toBe(5);
      expect(result.data.orderHint).toBe(0);
    }
  });

  it('rejects lineHint < 1', () => {
    const result = lspDefinitionSchema.safeParse({
      uri: '/project/src/index.ts',
      symbolName: 'MyClass',
      lineHint: '0',
    });
    expect(result.success).toBe(false);
  });

  it('rejects empty symbolName', () => {
    const result = lspDefinitionSchema.safeParse({
      uri: '/project/src/index.ts',
      symbolName: '',
      lineHint: '10',
    });
    expect(result.success).toBe(false);
  });

  it('rejects path traversal in uri', () => {
    const result = lspDefinitionSchema.safeParse({
      uri: '../../../etc/passwd',
      symbolName: 'test',
      lineHint: '10',
    });
    expect(result.success).toBe(false);
  });
});

describe('lspReferencesSchema', () => {
  it('parses with all defaults', () => {
    const result = lspReferencesSchema.safeParse({
      uri: '/project/src/index.ts',
      symbolName: 'MyClass',
      lineHint: '10',
    });
    expect(result.success).toBe(true);
    if (result.success) {
      expect(result.data.includeDeclaration).toBe(true);
      expect(result.data.contextLines).toBe(2);
      expect(result.data.referencesPerPage).toBe(20);
      expect(result.data.page).toBe(1);
    }
  });

  it('overrides defaults from strings', () => {
    const result = lspReferencesSchema.safeParse({
      uri: '/project/src/index.ts',
      symbolName: 'MyClass',
      lineHint: '10',
      includeDeclaration: 'false',
      referencesPerPage: '50',
      page: '3',
    });
    expect(result.success).toBe(true);
    if (result.success) {
      expect(result.data.includeDeclaration).toBe(false);
      expect(result.data.referencesPerPage).toBe(50);
      expect(result.data.page).toBe(3);
    }
  });
});

describe('lspCallsSchema', () => {
  it('requires direction', () => {
    const result = lspCallsSchema.safeParse({
      uri: '/project/src/index.ts',
      symbolName: 'myFn',
      lineHint: '10',
    });
    expect(result.success).toBe(false);
  });

  it('accepts valid direction', () => {
    const result = lspCallsSchema.safeParse({
      uri: '/project/src/index.ts',
      symbolName: 'myFn',
      lineHint: '10',
      direction: 'incoming',
    });
    expect(result.success).toBe(true);
    if (result.success) {
      expect(result.data.direction).toBe('incoming');
      expect(result.data.depth).toBe(1);
    }
  });

  it('rejects invalid direction', () => {
    const result = lspCallsSchema.safeParse({
      uri: '/project/src/index.ts',
      symbolName: 'myFn',
      lineHint: '10',
      direction: 'both',
    });
    expect(result.success).toBe(false);
  });
});


describe('githubSearchSchema', () => {
  it('parses comma-separated keywords', () => {
    const result = githubSearchSchema.safeParse({
      keywordsToSearch: 'react,hooks',
    });
    expect(result.success).toBe(true);
    if (result.success) {
      expect(result.data.keywordsToSearch).toEqual(['react', 'hooks']);
    }
  });
});

describe('githubContentSchema', () => {
  it('requires owner, repo, and path', () => {
    const missing = githubContentSchema.safeParse({ owner: 'fb', repo: 'react' });
    expect(missing.success).toBe(false);

    const valid = githubContentSchema.safeParse({
      owner: 'fb',
      repo: 'react',
      path: 'src/index.ts',
    });
    expect(valid.success).toBe(true);
  });
});

describe('githubReposSchema', () => {
  it('requires at least keywordsToSearch or topicsToSearch', () => {
    const empty = githubReposSchema.safeParse({});
    expect(empty.success).toBe(false);

    const withKeywords = githubReposSchema.safeParse({
      keywordsToSearch: 'react',
    });
    expect(withKeywords.success).toBe(true);

    const withTopics = githubReposSchema.safeParse({
      topicsToSearch: 'typescript',
    });
    expect(withTopics.success).toBe(true);
  });
});

describe('githubStructureSchema', () => {
  it('requires owner, repo, and branch', () => {
    const result = githubStructureSchema.safeParse({
      owner: 'fb',
      repo: 'react',
      branch: 'main',
    });
    expect(result.success).toBe(true);

    const missing = githubStructureSchema.safeParse({
      owner: 'fb',
      repo: 'react',
    });
    expect(missing.success).toBe(false);
  });

  it('converts depth from string', () => {
    const result = githubStructureSchema.safeParse({
      owner: 'fb',
      repo: 'react',
      branch: 'main',
      depth: '2',
    });
    expect(result.success).toBe(true);
    if (result.success) {
      expect(result.data.depth).toBe(2);
    }
  });
});

describe('githubPRsSchema', () => {
  it('parses with state enum', () => {
    const result = githubPRsSchema.safeParse({ state: 'open' });
    expect(result.success).toBe(true);
  });

  it('converts merged boolean from string', () => {
    const result = githubPRsSchema.safeParse({ merged: 'true' });
    expect(result.success).toBe(true);
    if (result.success) {
      expect(result.data.merged).toBe(true);
    }
  });

  it('converts prNumber from string', () => {
    const result = githubPRsSchema.safeParse({ prNumber: '123' });
    expect(result.success).toBe(true);
    if (result.success) {
      expect(result.data.prNumber).toBe(123);
    }
  });
});


describe('packageSearchSchema', () => {
  it('parses minimal input with defaults', () => {
    const result = packageSearchSchema.safeParse({ name: 'express' });
    expect(result.success).toBe(true);
    if (result.success) {
      expect(result.data.name).toBe('express');
      expect(result.data.ecosystem).toBe('npm');
    }
  });

  it('accepts python ecosystem', () => {
    const result = packageSearchSchema.safeParse({
      name: 'requests',
      ecosystem: 'python',
    });
    expect(result.success).toBe(true);
    if (result.success) {
      expect(result.data.ecosystem).toBe('python');
    }
  });

  it('rejects empty name', () => {
    const result = packageSearchSchema.safeParse({ name: '' });
    expect(result.success).toBe(false);
  });

  it('rejects invalid ecosystem', () => {
    const result = packageSearchSchema.safeParse({
      name: 'test',
      ecosystem: 'rubygems',
    });
    expect(result.success).toBe(false);
  });
});
