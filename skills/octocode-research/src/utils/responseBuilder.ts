import {
  createRoleBasedResult,
  QuickResult,
  StatusEmoji,
} from 'octocode-mcp/public';

type CallToolResult = ReturnType<typeof createRoleBasedResult>;


interface PaginationInfo {
  page: number;
  total: number;
  hasMore: boolean;
  perPage?: number;
  totalItems?: number;
}


interface MatchLocation {
  line: number;
  column?: number;
  value?: string;
  byteOffset?: number;
  charOffset?: number;
}


interface FileMatch {
  path: string;
  matches?: number;
  line?: number;
  preview?: string;
  repo?: string;
  
  allMatches?: MatchLocation[];
}


interface ResearchContext {
  mainResearchGoal?: string;
  researchGoal?: string;
  reasoning?: string;
}


export const ResearchResponse = {
  
  searchResults(results: {
    files: FileMatch[];
    totalMatches: number;
    pagination?: PaginationInfo;
    searchPattern?: string;
    isLocal?: boolean;
    
    mcpHints?: string[];
    
    research?: ResearchContext;
  }): CallToolResult {
    const { files, totalMatches, pagination, searchPattern, mcpHints = [] } = results;

    const patternInfo = searchPattern ? ` for "${searchPattern}"` : '';
    const summary =
      files.length > 0
        ? `Found ${totalMatches} matches${patternInfo} in ${files.length} files:\n` +
          files
            .slice(0, 10)
            .map(
              f =>
                `- ${f.repo ? `${f.repo}:` : ''}${f.path}${f.line ? ` (line ${f.line})` : ''}${f.matches ? ` [${f.matches} matches]` : ''}${typeof f.preview === 'string' && f.preview ? `\n  "${f.preview.slice(0, 100)}${f.preview.length > 100 ? '...' : ''}"` : ''}`
            )
            .join('\n') +
          (files.length > 10 ? `\n... and ${files.length - 10} more files` : '')
        : `No matches found${patternInfo}`;

    const hints: string[] = [...mcpHints];
    if (files.length > 10) {
      hints.push(`Showing 10 of ${files.length} files`);
    }
    if (pagination?.hasMore) {
      hints.push(`Next page: page=${pagination.page + 1}`);
    }

    if (files.length === 0) {
      const emptyHints = mcpHints.length > 0 ? mcpHints : [
        'Try broader search terms',
        'Check spelling and case sensitivity',
        'Remove path filters to widen search',
      ];
      return QuickResult.empty(summary, emptyHints);
    }

    if (pagination) {
      return QuickResult.paginated(summary, results, pagination, hints);
    }

    return QuickResult.success(summary, results, hints);
  },

  
  fileContent(result: {
    path: string;
    content: string;
    lines?: { start: number; end: number };
    language?: string;
    totalLines?: number;
    isPartial?: boolean;
    
    mcpHints?: string[];
    
    research?: ResearchContext;
  }): CallToolResult {
    const { path, content, lines, language, totalLines, isPartial, mcpHints = [] } = result;

    const lineInfo = lines ? ` (lines ${lines.start}-${lines.end})` : '';
    const lang = language || detectLanguage(path);

    const formattedContent = `📄 ${path}${lineInfo}\n\n\`\`\`${lang}\n${content}\n\`\`\``;

    const hints: string[] = [...mcpHints];
    hints.push('Content retrieved successfully');
    if (lines) {
      hints.push(`Showing lines ${lines.start}-${lines.end}`);
    }
    if (totalLines && isPartial) {
      hints.push(`File has ${totalLines} total lines`);
      hints.push('Use startLine/endLine for specific ranges');
    }

    return createRoleBasedResult({
      system: { hints },
      assistant: { summary: formattedContent, format: 'markdown' },
      user: { message: `Retrieved: ${path}`, emoji: StatusEmoji.file },
      data: result,
    });
  },

  
  lspResult(result: {
    symbol: string;
    locations: Array<{ uri: string; line: number; preview?: string }>;
    type: 'definition' | 'references' | 'calls' | 'incoming' | 'outgoing';
    
    mcpHints?: string[];
    
    research?: ResearchContext;
  }): CallToolResult {
    const { symbol, locations, type, mcpHints = [] } = result;

    const typeLabels: Record<string, string> = {
      definition: 'Definition',
      references: 'References',
      calls: 'Call sites',
      incoming: 'Incoming calls',
      outgoing: 'Outgoing calls',
    };
    const typeEmojis: Record<string, string> = {
      definition: StatusEmoji.definition,
      references: StatusEmoji.reference,
      calls: StatusEmoji.call,
      incoming: '📥',
      outgoing: '📤',
    };

    const typeLabel = typeLabels[type] || type;
    const typeEmoji = typeEmojis[type] || StatusEmoji.info;

    const summary =
      locations.length > 0
        ? `${typeLabel} for "${symbol}":\n` +
          locations
            .map(
              l =>
                `- ${l.uri}:${l.line}${l.preview ? `\n  ${l.preview}` : ''}`
            )
            .join('\n')
        : `No ${type} found for "${symbol}"`;

    const hints: string[] = [...mcpHints];
    if (locations.length > 0) {
      hints.push('Use returned line numbers for further navigation');
      if (type === 'definition') hints.push('Use lspFindReferences to find all usages');
      if (type === 'references') hints.push('Use lspCallHierarchy for call relationships');
    } else if (mcpHints.length === 0) {
      hints.push('Symbol may be external or unindexed');
      hints.push('Try localSearchCode as fallback');
      hints.push('Check if file is in workspace');
    }

    return createRoleBasedResult({
      system: { hints },
      assistant: { summary },
      user: {
        message: `${typeLabel}: ${locations.length} found`,
        emoji: locations.length > 0 ? typeEmoji : StatusEmoji.empty,
      },
      data: result,
    });
  },

  
  repoStructure(result: {
    path: string;
    structure: { files: string[]; folders: string[] };
    depth?: number;
    totalFiles?: number;
    totalFolders?: number;
    owner?: string;
    repo?: string;
    
    mcpHints?: string[];
    
    research?: ResearchContext;
  }): CallToolResult {
    const { path, structure, depth, totalFiles, totalFolders, owner, repo, mcpHints = [] } =
      result;

    const repoInfo = owner && repo ? `${owner}/${repo}` : '';
    const pathInfo = path || '/';

    const fileList = structure.files.slice(0, 20);
    const folderList = structure.folders.slice(0, 20);

    const summary =
      `📁 ${repoInfo ? `${repoInfo}:` : ''}${pathInfo}\n\n` +
      (folderList.length > 0
        ? `Folders:\n${folderList.map(f => `  📁 ${f}`).join('\n')}\n\n`
        : '') +
      (fileList.length > 0
        ? `Files:\n${fileList.map(f => `  📄 ${f}`).join('\n')}`
        : 'No files in this directory');

    const hints: string[] = [...mcpHints];
    if (depth === 1) {
      hints.push('Use depth=2 to see nested contents');
    }
    if (structure.files.length > 20 || structure.folders.length > 20) {
      hints.push('Results truncated - use path filter to narrow scope');
    }
    hints.push('Use localSearchCode or githubSearchCode to find specific files');

    return createRoleBasedResult({
      system: {
        hints,
        pagination:
          totalFiles || totalFolders
            ? {
                currentPage: 1,
                totalPages: 1,
                hasMore: false,
                totalItems: (totalFiles || 0) + (totalFolders || 0),
              }
            : undefined,
      },
      assistant: { summary, format: 'markdown' },
      user: {
        message: `${structure.files.length} files, ${structure.folders.length} folders`,
        emoji: StatusEmoji.folder,
      },
      data: result,
    });
  },

  
  packageSearch(result: {
    packages: Array<{
      name: string;
      version?: string;
      description?: string;
      repository?: string;
    }>;
    registry: 'npm' | 'pypi';
    query?: string;
    
    mcpHints?: string[];
    
    research?: ResearchContext;
  }): CallToolResult {
    const { packages, registry, query, mcpHints = [] } = result;

    const queryInfo = query ? ` for "${query}"` : '';
    const summary =
      packages.length > 0
        ? `Found ${packages.length} packages${queryInfo} on ${registry.toUpperCase()}:\n` +
          packages
            .slice(0, 10)
            .map(
              p =>
                `- ${p.name}${p.version ? `@${p.version}` : ''}\n  ${typeof p.description === 'string' ? p.description : 'No description'}${p.repository ? `\n  ${p.repository}` : ''}`
            )
            .join('\n')
        : `No packages found${queryInfo} on ${registry.toUpperCase()}`;

    const hints: string[] = [...mcpHints];
    if (packages.length > 0) {
      hints.push('Use repository URL with githubViewRepoStructure to explore source');
      hints.push('Use githubSearchCode to find usage examples');
    } else if (mcpHints.length === 0) {
      hints.push('Try different search terms');
      hints.push('Check package name spelling');
    }

    if (packages.length === 0) {
      return QuickResult.empty(summary, hints);
    }

    return QuickResult.success(summary, result, hints);
  },

  
  pullRequests(result: {
    prs: Array<{
      number: number;
      title: string;
      state: string;
      author?: string;
      url?: string;
    }>;
    repo?: string;
    pagination?: PaginationInfo;
    
    mcpHints?: string[];
    
    research?: ResearchContext;
  }): CallToolResult {
    const { prs, repo, pagination, mcpHints = [] } = result;

    const repoInfo = repo ? ` in ${repo}` : '';
    const summary =
      prs.length > 0
        ? `Found ${prs.length} pull requests${repoInfo}:\n` +
          prs
            .slice(0, 10)
            .map(
              pr =>
                `- #${pr.number}: ${pr.title} [${pr.state}]${pr.author ? ` by @${pr.author}` : ''}`
            )
            .join('\n')
        : `No pull requests found${repoInfo}`;

    const hints: string[] = [...mcpHints];
    if (prs.length > 0) {
      hints.push('Use prNumber with type="fullContent" to see full diff');
      hints.push('Use type="partialContent" with file filter for specific changes');
    } else if (mcpHints.length === 0) {
      hints.push('Try broader date range');
      hints.push('Check repository name');
    }

    if (prs.length === 0) {
      return QuickResult.empty(summary, hints);
    }

    if (pagination) {
      return QuickResult.paginated(summary, result, pagination, hints);
    }

    return QuickResult.success(summary, result, hints);
  },

  
  bulkResult(result: {
    results: Array<{ status: string; data?: unknown; error?: string }>;
    operation: string;
    totalQueries: number;
  }): CallToolResult {
    const { results, operation, totalQueries } = result;

    const successful = results.filter(r => r.status === 'success').length;
    const failed = results.filter(r => r.status === 'error').length;
    const empty = results.filter(r => r.status === 'empty').length;

    const summary =
      `Bulk ${operation} completed:\n` +
      `- ✅ Success: ${successful}/${totalQueries}\n` +
      (empty > 0 ? `- 📭 Empty: ${empty}/${totalQueries}\n` : '') +
      (failed > 0 ? `- ❌ Failed: ${failed}/${totalQueries}` : '');

    const hints: string[] = [];
    if (failed > 0) {
      hints.push('Check individual error messages for failed queries');
    }
    if (empty > 0) {
      hints.push('Empty results may indicate no matches or invalid parameters');
    }

    const emoji =
      failed === 0
        ? StatusEmoji.success
        : failed === totalQueries
          ? StatusEmoji.error
          : StatusEmoji.partial;

    return createRoleBasedResult({
      system: { hints },
      assistant: { summary },
      user: {
        message: `${successful}/${totalQueries} queries succeeded`,
        emoji,
      },
      data: result,
      isError: failed === totalQueries,
    });
  },
};


function detectLanguage(path: string): string {
  const ext = path.split('.').pop()?.toLowerCase() || '';
  const langMap: Record<string, string> = {
    ts: 'typescript',
    tsx: 'typescript',
    js: 'javascript',
    jsx: 'javascript',
    py: 'python',
    rb: 'ruby',
    go: 'go',
    rs: 'rust',
    java: 'java',
    kt: 'kotlin',
    swift: 'swift',
    c: 'c',
    cpp: 'cpp',
    h: 'c',
    hpp: 'cpp',
    cs: 'csharp',
    php: 'php',
    sh: 'bash',
    bash: 'bash',
    zsh: 'bash',
    yml: 'yaml',
    yaml: 'yaml',
    json: 'json',
    md: 'markdown',
    sql: 'sql',
    html: 'html',
    css: 'css',
    scss: 'scss',
    less: 'less',
  };
  return langMap[ext] || '';
}

export { QuickResult, detectLanguage as detectLanguageFromPath };
