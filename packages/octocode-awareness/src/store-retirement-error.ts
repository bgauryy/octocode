export class StoreRetirementError extends Error {
  constructor(readonly code: string, message: string) {
    super(message);
    this.name = 'StoreRetirementError';
  }
}
