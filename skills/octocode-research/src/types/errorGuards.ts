
interface ErrorWithStatus {
  status: number;
  message?: string;
}

interface ErrorWithCode {
  code: string;
  message?: string;
}

function isErrorWithStatus(err: unknown): err is ErrorWithStatus {
  return (
    err !== null &&
    typeof err === 'object' &&
    'status' in err &&
    typeof (err as ErrorWithStatus).status === 'number'
  );
}

function isErrorWithCode(err: unknown): err is ErrorWithCode {
  return (
    err !== null &&
    typeof err === 'object' &&
    'code' in err &&
    typeof (err as ErrorWithCode).code === 'string'
  );
}

function hasMessage(err: unknown): err is { message: string } {
  return (
    err !== null &&
    typeof err === 'object' &&
    'message' in err &&
    typeof (err as { message: unknown }).message === 'string'
  );
}


export function getErrorStatus(err: unknown): number | undefined {
  return isErrorWithStatus(err) ? err.status : undefined;
}

function getErrorCode(err: unknown): string | undefined {
  return isErrorWithCode(err) ? err.code : undefined;
}

function getErrorMessage(err: unknown): string | undefined {
  return hasMessage(err) ? err.message : undefined;
}

export function hasStatusIn(err: unknown, statuses: readonly number[]): boolean {
  const status = getErrorStatus(err);
  return status !== undefined && statuses.includes(status);
}

export function hasCodeIn(err: unknown, codes: readonly string[]): boolean {
  const code = getErrorCode(err);
  return code !== undefined && codes.includes(code);
}

export function messageMatches(err: unknown, patterns: readonly RegExp[]): boolean {
  const message = getErrorMessage(err);
  return message !== undefined && patterns.some((p) => p.test(message));
}
