import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { CallToolResult } from '@modelcontextprotocol/sdk/types.js';

const mockSanitizeContent = vi.fn();
const mockSanitizeStructuredContent = vi.fn();

vi.mock('@octocodeai/octocode-engine/security', () => ({
  ContentSanitizer: {
    sanitizeContent: mockSanitizeContent,
  },
}));

vi.mock('../../../src/responses.js', () => ({
  sanitizeStructuredContent: mockSanitizeStructuredContent,
}));

const { sanitizeCallToolResult } = await import(
  '../../../src/utils/response/callToolResult.js'
);
const { setRuntimeSurface, _resetRuntimeSurface } = await import('@octocodeai/config');

const AWS_KEY = 'AKIAIOSFODNN7EXAMPLE';
const REDACTED = '[REDACTED-AWS_ACCESS_KEY_ID]';

describe('sanitizeCallToolResult', () => {
  beforeEach(() => {
    mockSanitizeContent.mockReset();
    mockSanitizeStructuredContent.mockReset();
    _resetRuntimeSurface();
    delete process.env.OCTOCODE_MCP_FULL_TEXT;
  });

  afterEach(() => {
    _resetRuntimeSurface();
    delete process.env.OCTOCODE_MCP_FULL_TEXT;
    vi.restoreAllMocks();
  });

  it('redacts secrets — sanitizeContent result used directly, secret absent from output', () => {
    const input = `config: ${AWS_KEY}`;
    mockSanitizeContent.mockReturnValue({
      content: `config: ${REDACTED}`,
      hasSecrets: true,
      secretsDetected: ['AWS_ACCESS_KEY_ID'],
      warnings: [],
    });
    mockSanitizeStructuredContent.mockImplementation((obj: unknown) => obj);

    const result = sanitizeCallToolResult({
      content: [{ type: 'text', text: input }],
    } as CallToolResult);

    const textBlock = result.content?.[0];
    expect(textBlock && 'text' in textBlock ? textBlock.text : '').not.toContain(AWS_KEY);
    expect(textBlock && 'text' in textBlock ? textBlock.text : '').toContain(REDACTED);
  });

  it('passes clean text through unchanged when no secrets detected', () => {
    const clean = 'no secrets here';
    mockSanitizeContent.mockReturnValue({
      content: clean,
      hasSecrets: false,
      secretsDetected: [],
      warnings: [],
    });
    mockSanitizeStructuredContent.mockImplementation((obj: unknown) => obj);

    const result = sanitizeCallToolResult({
      content: [{ type: 'text', text: clean }],
    } as CallToolResult);

    const textBlock = result.content?.[0];
    expect(textBlock && 'text' in textBlock ? textBlock.text : '').toBe(clean);
  });

  it('calls the native scanner exactly once per text item (no redundant second pass)', () => {
    const blob = 'x'.repeat(50_000);
    mockSanitizeContent.mockReturnValue({
      content: blob,
      hasSecrets: false,
      secretsDetected: [],
      warnings: [],
    });
    mockSanitizeStructuredContent.mockImplementation((obj: unknown) => obj);

    sanitizeCallToolResult({
      content: [{ type: 'text', text: blob }],
    } as CallToolResult);

    expect(mockSanitizeContent).toHaveBeenCalledTimes(1);
  });

  it('calls the native scanner once per item across multiple text blocks', () => {
    mockSanitizeContent.mockReturnValue({
      content: 'clean',
      hasSecrets: false,
      secretsDetected: [],
      warnings: [],
    });
    mockSanitizeStructuredContent.mockImplementation((obj: unknown) => obj);

    sanitizeCallToolResult({
      content: [
        { type: 'text', text: 'a' },
        { type: 'text', text: 'b' },
        { type: 'text', text: 'c' },
      ],
    } as CallToolResult);

    expect(mockSanitizeContent).toHaveBeenCalledTimes(3);
  });

  it('sanitizes structuredContent independently of text blocks', () => {
    mockSanitizeContent.mockReturnValue({
      content: 'text',
      hasSecrets: false,
      secretsDetected: [],
      warnings: [],
    });
    const sanitizedStructured = { status: 'ok' };
    mockSanitizeStructuredContent.mockReturnValue(sanitizedStructured);

    const result = sanitizeCallToolResult({
      content: [{ type: 'text', text: 'text' }],
      structuredContent: { status: 'ok', raw: `secret: ${AWS_KEY}` },
    } as unknown as CallToolResult);

    expect(mockSanitizeStructuredContent).toHaveBeenCalledTimes(1);
    expect(result.structuredContent).toEqual(sanitizedStructured);
  });

  it('compacts duplicate text blocks on the MCP surface when structuredContent exists', () => {
    mockSanitizeContent.mockReturnValue({
      content: 'should not scan duplicate yaml',
      hasSecrets: false,
      secretsDetected: [],
      warnings: [],
    });
    mockSanitizeStructuredContent.mockImplementation((obj: unknown) => obj);

    const result = sanitizeCallToolResult({
      content: [{ type: 'text', text: 'results:\n- huge duplicate yaml' }],
      structuredContent: {
        results: [
          { id: 'q1', data: { path: 'a.ts' } },
          { id: 'q2', status: 'empty', data: {} },
        ],
        pagination: { hasMore: false },
      },
    } as unknown as CallToolResult);

    const textBlock = result.content?.[0];
    expect(textBlock && 'text' in textBlock ? textBlock.text : '').toBe(
      'structuredContent available · results=2 · empty=1 · hasMore=false. Read structuredContent for full data.'
    );
    expect(mockSanitizeContent).not.toHaveBeenCalled();
    expect(result.structuredContent).toEqual({
      results: [
        { id: 'q1', data: { path: 'a.ts' } },
        { id: 'q2', status: 'empty', data: {} },
      ],
      pagination: { hasMore: false },
    });
  });

  it('preserves full text blocks on the CLI surface', () => {
    setRuntimeSurface('cli');
    mockSanitizeContent.mockReturnValue({
      content: 'results:\n- full cli yaml',
      hasSecrets: false,
      secretsDetected: [],
      warnings: [],
    });
    mockSanitizeStructuredContent.mockImplementation((obj: unknown) => obj);

    const result = sanitizeCallToolResult({
      content: [{ type: 'text', text: 'results:\n- full cli yaml' }],
      structuredContent: { results: [{ id: 'q1' }] },
    } as unknown as CallToolResult);

    const textBlock = result.content?.[0];
    expect(textBlock && 'text' in textBlock ? textBlock.text : '').toBe(
      'results:\n- full cli yaml'
    );
    expect(mockSanitizeContent).toHaveBeenCalledTimes(1);
  });

  it('preserves full MCP text blocks when the escape hatch is enabled', () => {
    process.env.OCTOCODE_MCP_FULL_TEXT = 'true';
    mockSanitizeContent.mockReturnValue({
      content: 'results:\n- full mcp yaml',
      hasSecrets: false,
      secretsDetected: [],
      warnings: [],
    });
    mockSanitizeStructuredContent.mockImplementation((obj: unknown) => obj);

    const result = sanitizeCallToolResult({
      content: [{ type: 'text', text: 'results:\n- full mcp yaml' }],
      structuredContent: { results: [{ id: 'q1' }] },
    } as unknown as CallToolResult);

    const textBlock = result.content?.[0];
    expect(textBlock && 'text' in textBlock ? textBlock.text : '').toBe(
      'results:\n- full mcp yaml'
    );
    expect(mockSanitizeContent).toHaveBeenCalledTimes(1);
  });

  it('preserves error text even when structuredContent exists', () => {
    mockSanitizeContent.mockReturnValue({
      content: 'error: sanitized detail',
      hasSecrets: false,
      secretsDetected: [],
      warnings: [],
    });
    mockSanitizeStructuredContent.mockImplementation((obj: unknown) => obj);

    const result = sanitizeCallToolResult({
      content: [{ type: 'text', text: 'error: original detail' }],
      structuredContent: { status: 'error' },
      isError: true,
    } as unknown as CallToolResult);

    const textBlock = result.content?.[0];
    expect(textBlock && 'text' in textBlock ? textBlock.text : '').toBe(
      'error: sanitized detail'
    );
    expect(mockSanitizeContent).toHaveBeenCalledTimes(1);
  });

  it('returns item unchanged on sanitizeContent throw', () => {
    mockSanitizeContent.mockImplementation(() => {
      throw new Error('native error');
    });
    mockSanitizeStructuredContent.mockImplementation((obj: unknown) => obj);

    const original = { type: 'text' as const, text: 'fallback text' };
    const result = sanitizeCallToolResult({
      content: [original],
    } as CallToolResult);

    const textBlock = result.content?.[0];
    expect(textBlock && 'text' in textBlock ? textBlock.text : '').toBe('fallback text');
  });
});
