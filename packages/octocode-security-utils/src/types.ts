export interface SensitiveDataPattern {
  name: string;
  description: string;
  regex: RegExp;
  fileContext?: RegExp;
  matchAccuracy?: 'high' | 'medium';
}

export interface SanitizationResult {
  content: string;
  hasSecrets: boolean;
  secretsDetected: string[];
  warnings: string[];
}

export interface ValidationResult {
  sanitizedParams: Record<string, unknown>;
  isValid: boolean;
  hasSecrets: boolean;
  warnings: string[];
}

export interface PathValidationResult {
  isValid: boolean;
  error?: string;
  sanitizedPath?: string;
}

export interface ToolResult {
  content: Array<{ type: string; text?: string }>;
  isError?: boolean;
}

export interface ISanitizer {
  sanitizeContent(content: string, filePath?: string): SanitizationResult;
  validateInputParameters(params: Record<string, unknown>): ValidationResult;
}
