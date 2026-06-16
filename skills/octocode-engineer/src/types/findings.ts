import type { AnalysisSignal, RecommendedValidation } from './analysis.js';

export type { AnalysisLens, AnalysisSignal, RecommendedValidation, FlowTraceStep } from './analysis.js';

export interface PillarFindingStats {
  totalFindings: number;
  severityBreakdown: Record<string, number>;
}

export interface FindingStats {
  overall?: PillarFindingStats;
  pillars?: Record<string, PillarFindingStats>;
}

interface SuggestedFix {
  strategy: string;
  steps: string[];
}

interface LspHint {
  tool: 'lspGetSemantics';
  semanticType: 'definition' | 'references' | 'callers' | 'callees' | 'callHierarchy';
  symbolName: string;
  lineHint: number;
  file: string;
  expectedResult: string;
}

export interface Finding {
  id: string;
  severity: 'critical' | 'high' | 'medium' | 'low' | 'info';
  category: string;
  file: string;
  lineStart: number;
  lineEnd: number;
  title: string;
  reason: string;
  files: string[];
  suggestedFix: SuggestedFix;
  impact?: string;
  tags?: string[];
  columnStart?: number;
  columnEnd?: number;
  lspHints?: LspHint[];
  ruleId?: string;
  analysisLens?: import('./analysis.js').AnalysisLens;
  confidence?: 'high' | 'medium' | 'low';
  evidence?: Record<string, unknown>;
  correlatedSignals?: string[];
  recommendedValidation?: RecommendedValidation;
  flowTrace?: import('./analysis.js').FlowTraceStep[];
}

export interface TopRecommendation {
  id: string;
  file: string;
  severity: string;
  category: string;
  title: string;
  reason: string;
  suggestedFix?: Finding['suggestedFix'];
}

export interface ScanSummaryData {
  totalPackages?: number;
  totalFiles?: number;
  totalNodes?: number;
  totalFunctions?: number;
  totalFlows?: number;
  totalDependencyFiles?: number;
  byPackage?: Record<
    string,
    {
      files: number;
      nodes: number;
      functions: number;
      flows: number;
      topKinds: [string, number][];
      rootPath: string;
    }
  >;
  [key: string]: unknown;
}

export interface AgentOutputData {
  totalFindings?: number;
  totalBeforeTruncation?: number;
  droppedCategories?: string[];
  findingStats?: FindingStats | null;
  analysisSummary?: {
    strongestGraphSignal?: AnalysisSignal | null;
    strongestAstSignal?: AnalysisSignal | null;
    combinedSignals?: AnalysisSignal[];
    recommendedValidation?: RecommendedValidation | null;
  };
  highPriority?: number;
  mediumPriority?: number;
  lowPriority?: number;
  topRecommendations?: TopRecommendation[];
  filesWithIssues?: Array<{
    file: string;
    issueCount: number;
    issueIds: string[];
  }>;
  [key: string]: unknown;
}
