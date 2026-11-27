/** The message of a thrown value: an Error's message, otherwise the value as a string. */
export function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
