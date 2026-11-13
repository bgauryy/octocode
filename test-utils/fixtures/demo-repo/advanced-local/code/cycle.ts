export function alpha(n: number): number {
  if (n <= 0) return 0;
  return beta(n - 1) + gamma(n - 1);
}

export function beta(n: number): number {
  return alpha(n - 1);
}

export function gamma(n: number): number {
  return beta(n - 1);
}
