import { describe, expect, it, vi } from 'vitest';

const mockSanitizeContent = vi.hoisted(() => vi.fn());
const mockMaskSensitiveData = vi.hoisted(() => vi.fn());

vi.mock('@octocodeai/octocode-engine/contentSanitizer', () => ({
  ContentSanitizer: { sanitizeContent: mockSanitizeContent },
}));

vi.mock('@octocodeai/octocode-engine/mask', () => ({
  maskSensitiveData: mockMaskSensitiveData,
}));

const { sanitizeContent, maskSensitiveData } = await import(
  '../../src/security/sanitize.js'
);

describe('sanitize', () => {
  describe('sanitizeContent', () => {
    it('delegates to ContentSanitizer.sanitizeContent with content only', () => {
      const result = { content: 'clean', warnings: [], redacted: false };
      mockSanitizeContent.mockReturnValueOnce(result);

      expect(sanitizeContent('raw text')).toBe(result);
      expect(mockSanitizeContent).toHaveBeenCalledWith('raw text', undefined);
    });

    it('passes filePath through when provided', () => {
      const result = { content: 'clean', warnings: [], redacted: false };
      mockSanitizeContent.mockReturnValueOnce(result);

      expect(sanitizeContent('raw text', '/src/foo.ts')).toBe(result);
      expect(mockSanitizeContent).toHaveBeenCalledWith(
        'raw text',
        '/src/foo.ts'
      );
    });

    it('propagates the full SanitizationResult including warnings', () => {
      const result = {
        content: 'clean',
        warnings: ['redacted secret'],
        redacted: true,
      };
      mockSanitizeContent.mockReturnValueOnce(result);

      const out = sanitizeContent('secret text');
      expect(out.warnings).toEqual(['redacted secret']);
      expect(out.redacted).toBe(true);
    });
  });

  describe('maskSensitiveData', () => {
    it('re-exports the engine maskSensitiveData function', () => {
      mockMaskSensitiveData.mockReturnValueOnce('***');
      expect(maskSensitiveData('secret')).toBe('***');
      expect(mockMaskSensitiveData).toHaveBeenCalledWith('secret');
    });
  });
});
