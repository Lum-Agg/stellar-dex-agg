export function isWalletModalDismissal(error: unknown): boolean {
  if (!error || typeof error !== 'object') return false;
  const candidate = error as { code?: unknown; message?: unknown };
  return candidate.code === -1 && candidate.message === 'The user closed the modal.';
}
