# auth.ts — fixture for extract/summarize evals
export function createToken(userId: string): string {
  return `tok_${userId}`;
}

export function revokeToken(token: string): boolean {
  return token.startsWith("tok_");
}
