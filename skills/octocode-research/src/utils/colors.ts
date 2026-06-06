const colors = {
  reset: '\x1b[0m',
  
  agent: '\x1b[35m',
  agentBright: '\x1b[95m',
  
  result: '\x1b[34m',
  resultBright: '\x1b[94m',
  
  success: '\x1b[32m',
  error: '\x1b[31m',
  warn: '\x1b[33m',
  
  dim: '\x1b[2m',
};

export function agentLog(message: string): string {
  return `${colors.agentBright}${message}${colors.reset}`;
}

export function resultLog(message: string): string {
  return `${colors.resultBright}${message}${colors.reset}`;
}

export function successLog(message: string): string {
  return `${colors.success}${message}${colors.reset}`;
}

export function errorLog(message: string): string {
  return `${colors.error}${message}${colors.reset}`;
}

export function warnLog(message: string): string {
  return `${colors.warn}${message}${colors.reset}`;
}

export function dimLog(message: string): string {
  return `${colors.dim}${message}${colors.reset}`;
}
