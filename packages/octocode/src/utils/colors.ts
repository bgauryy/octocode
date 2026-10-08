const colors = {
  reset: '\x1b[0m',
  bright: '\x1b[1m',
  dim: '\x1b[2m',
  red: '\x1b[31m',
  green: '\x1b[32m',
  yellow: '\x1b[33m',
  cyan: '\x1b[36m',
} as const;

type ColorName = keyof typeof colors;

function colorsEnabled(): boolean {
  if (process.env.NO_COLOR) {
    return false;
  }
  return Boolean(process.stdout.isTTY);
}

export const c = (color: ColorName, text: string): string =>
  colorsEnabled() ? `${colors[color]}${text}${colors.reset}` : text;

export const bold = (text: string): string => c('bright', text);

export const dim = (text: string): string => c('dim', text);
