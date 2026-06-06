import yaml from 'js-yaml';

interface McpToolResponse {
  content?: Array<{ type: string; text?: string }>;
  structuredContent?: Record<string, unknown>;
  isError?: boolean;
}

interface ResearchContext {
  mainResearchGoal?: string;
  researchGoal?: string;
  reasoning?: string;
}

interface BulkResultItem {
  id: number;
  status: 'hasResults' | 'empty' | 'error';
  data: Record<string, unknown>;
  research: ResearchContext;
}


export interface ParsedResponse {
  data: Record<string, unknown>;
  isError: boolean;
  
  hints: string[];
  
  research: ResearchContext;
  
  status: 'hasResults' | 'empty' | 'error' | 'unknown';
}

interface ParsedBulkResponse {
  
  results: BulkResultItem[];
  
  hints: {
    hasResults: string[];
    empty: string[];
    error: string[];
  };
  
  instructions: string;
  
  isError: boolean;
  
  counts: {
    total: number;
    hasResults: number;
    empty: number;
    error: number;
  };
}


export function parseToolResponse(response: McpToolResponse): ParsedResponse {
  const emptyResult: ParsedResponse = {
    data: {},
    isError: true,
    hints: [],
    research: {},
    status: 'unknown',
  };

  if (response.structuredContent && typeof response.structuredContent === 'object') {
    return {
      data: response.structuredContent,
      isError: Boolean(response.isError),
      hints: [],
      research: {},
      status: 'unknown',
    };
  }

  if (response.content && response.content[0]?.text) {
    try {
      const parsed = yaml.load(response.content[0].text) as Record<string, unknown>;

      let hints: string[] = [];
      if (Array.isArray(parsed.hasResultsStatusHints)) {
        hints = parsed.hasResultsStatusHints as string[];
      } else if (Array.isArray(parsed.emptyStatusHints)) {
        hints = parsed.emptyStatusHints as string[];
      } else if (Array.isArray(parsed.errorStatusHints)) {
        hints = parsed.errorStatusHints as string[];
      }

      if (parsed && Array.isArray(parsed.results) && parsed.results.length > 0) {
        const firstResult = parsed.results[0] as Record<string, unknown>;
        const resultStatus = String(firstResult.status || 'unknown');

        const research: ResearchContext = {
          mainResearchGoal: typeof firstResult.mainResearchGoal === 'string'
            ? firstResult.mainResearchGoal : undefined,
          researchGoal: typeof firstResult.researchGoal === 'string'
            ? firstResult.researchGoal : undefined,
          reasoning: typeof firstResult.reasoning === 'string'
            ? firstResult.reasoning : undefined,
        };

        if (firstResult.data && typeof firstResult.data === 'object') {
          return {
            data: firstResult.data as Record<string, unknown>,
            isError: resultStatus === 'error',
            hints,
            research,
            status: resultStatus as ParsedResponse['status'],
          };
        }
      }

      return {
        data: parsed || {},
        isError: Boolean(response.isError),
        hints,
        research: {},
        status: 'unknown',
      };
    } catch {
      return emptyResult;
    }
  }

  return emptyResult;
}

export function parseToolResponseBulk(response: McpToolResponse): ParsedBulkResponse {
  const emptyResult: ParsedBulkResponse = {
    results: [],
    hints: { hasResults: [], empty: [], error: [] },
    instructions: '',
    isError: true,
    counts: { total: 0, hasResults: 0, empty: 0, error: 0 },
  };

  if (!response.content || !response.content[0]?.text) {
    return emptyResult;
  }

  try {
    const parsed = yaml.load(response.content[0].text) as Record<string, unknown>;

    if (!parsed || !Array.isArray(parsed.results)) {
      return emptyResult;
    }

    const results: BulkResultItem[] = [];
    let hasResultsCount = 0;
    let emptyCount = 0;
    let errorCount = 0;

    for (const result of parsed.results) {
      if (!result || typeof result !== 'object') continue;

      const r = result as Record<string, unknown>;
      const status = String(r.status || 'unknown') as BulkResultItem['status'];

      if (status === 'hasResults') hasResultsCount++;
      else if (status === 'empty') emptyCount++;
      else if (status === 'error') errorCount++;

      results.push({
        id: typeof r.id === 'number' ? r.id : results.length + 1,
        status,
        data: (r.data && typeof r.data === 'object' ? r.data : {}) as Record<string, unknown>,
        research: {
          mainResearchGoal: typeof r.mainResearchGoal === 'string' ? r.mainResearchGoal : undefined,
          researchGoal: typeof r.researchGoal === 'string' ? r.researchGoal : undefined,
          reasoning: typeof r.reasoning === 'string' ? r.reasoning : undefined,
        },
      });
    }

    const hints = {
      hasResults: Array.isArray(parsed.hasResultsStatusHints)
        ? (parsed.hasResultsStatusHints as string[])
        : [],
      empty: Array.isArray(parsed.emptyStatusHints)
        ? (parsed.emptyStatusHints as string[])
        : [],
      error: Array.isArray(parsed.errorStatusHints)
        ? (parsed.errorStatusHints as string[])
        : [],
    };

    return {
      results,
      hints,
      instructions: typeof parsed.instructions === 'string' ? parsed.instructions : '',
      isError: errorCount === results.length && results.length > 0,
      counts: {
        total: results.length,
        hasResults: hasResultsCount,
        empty: emptyCount,
        error: errorCount,
      },
    };
  } catch {
    return emptyResult;
  }
}
