import { describe, it, expect, vi, beforeAll } from 'vitest';
import { executeLspGetDiagnostics } from '../../src/tools/lsp/diagnostics/execution.js';
import { withResponseEnvelope } from '../../src/scheme/responseEnvelope.js';
import { LspGetDiagnosticsOutputSchema } from '../../src/tools/lsp/diagnostics/outputSchema.js';
import * as manager from '../../src/lsp/manager.js';
import { DiagnosticSeverity } from 'vscode-languageserver-protocol';

vi.mock('../../src/lsp/manager.js', async (i) => {
  const actual = await i<typeof import('../../src/lsp/manager.js')>();
  return { ...actual, isLanguageServerAvailable: vi.fn(), acquirePooledClient: vi.fn() };
});

describe('diagnostics severity - real errors', () => {
  it('converts numeric severity to string before schema validation', async () => {
    vi.mocked(manager.isLanguageServerAvailable).mockResolvedValue(true);
    vi.mocked(manager.acquirePooledClient).mockResolvedValue({
      getDiagnostics: vi.fn().mockResolvedValue({
        diagnostics: [
          {
            range: { start: { line: 0, character: 6 }, end: { line: 0, character: 7 } },
            severity: DiagnosticSeverity.Error,   // numeric: 1
            message: "Type 'string' is not assignable to type 'number'.",
            source: 'typescript',
            code: 2322,
          },
          {
            range: { start: { line: 1, character: 6 }, end: { line: 1, character: 7 } },
            severity: DiagnosticSeverity.Warning, // numeric: 2
            message: 'Some warning',
            source: 'typescript',
          },
        ],
        source: 'push',
      }),
    } as any);

    const result = await executeLspGetDiagnostics({
      queries: [{
        id: 'test-severity',
        mainResearchGoal: 'test',
        researchGoal: 'test',
        reasoning: 'test',
        uri: 'file:///Users/guybary/Documents/octocode-mcp/packages/octocode-mcp/src/index.ts',
      }],
    } as any);

    const sc = result.structuredContent as any;
    const diagnostics = sc?.results?.[0]?.data?.diagnostics;
    console.log('diagnostics:', JSON.stringify(diagnostics));

    // Validate schema
    const schema = withResponseEnvelope(LspGetDiagnosticsOutputSchema);
    const parsed = schema.safeParse(sc);
    if (!parsed.success) {
      console.log('SCHEMA ERRORS:', JSON.stringify(parsed.error.issues.slice(0, 3), null, 2));
    }
    expect(parsed.success).toBe(true);
    expect(diagnostics?.[0]?.severity).toBe('error');
    expect(diagnostics?.[1]?.severity).toBe('warning');
  });
});
