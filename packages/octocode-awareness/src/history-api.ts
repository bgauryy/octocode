/** Load the optional history backend only when history is requested. */
export const runAwarenessHistoryOperation: typeof import('./history.js').runAwarenessHistoryOperation = async (...args) => {
  const history = await import('./history.js');
  return history.runAwarenessHistoryOperation(...args);
};
