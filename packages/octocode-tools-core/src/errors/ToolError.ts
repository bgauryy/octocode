import {
  LOCAL_TOOL_ERROR_CODES,
  LOCAL_TOOL_ERROR_REGISTRY,
  LocalToolErrorCategory,
  type LocalToolErrorCode,
} from './localToolErrors.js';

export class ToolError extends Error {
  public readonly errorCode: LocalToolErrorCode;
  public readonly category: LocalToolErrorCategory;
  public readonly recoverability:
    | 'recoverable'
    | 'unrecoverable'
    | 'user-action-required';
  public readonly context?: Record<string, unknown>;

  constructor(
    errorCode: LocalToolErrorCode,
    message?: string,
    context?: Record<string, unknown>,
    cause?: Error
  ) {
    const metadata = LOCAL_TOOL_ERROR_REGISTRY[errorCode];
    const finalMessage = message || metadata.description;

    super(finalMessage, cause ? { cause } : undefined);

    this.name = 'ToolError';
    this.errorCode = errorCode;
    this.category = metadata.category;
    this.recoverability = metadata.recoverability;
    this.context = context;

    if (cause && cause.stack) {
      this.stack = `${this.stack}\nCaused by: ${cause.stack}`;
    }

    Object.setPrototypeOf(this, ToolError.prototype);
  }

  isRecoverable(): boolean {
    return this.recoverability === 'recoverable';
  }

  requiresUserAction(): boolean {
    return this.recoverability === 'user-action-required';
  }

  toJSON() {
    return {
      name: this.name,
      errorCode: this.errorCode,
      category: this.category,
      message: this.message,
      recoverability: this.recoverability,
      context: this.context,
      stack: this.stack,
    };
  }
}

export function isToolError(error: unknown): error is ToolError {
  return error instanceof ToolError;
}

export function toToolError(
  error: unknown,
  defaultErrorCode: LocalToolErrorCode = LOCAL_TOOL_ERROR_CODES.TOOL_EXECUTION_FAILED,
  context?: Record<string, unknown>
): ToolError {
  if (isToolError(error)) {
    return error;
  }

  if (error instanceof Error) {
    return new ToolError(defaultErrorCode, error.message, context, error);
  }

  const message = String(error);
  return new ToolError(defaultErrorCode, message, context);
}
