import { describe, expect, it } from 'vitest';
import { isWalletModalDismissal } from './wallet-errors';

describe('isWalletModalDismissal', () => {
  it('recognizes the Wallet Kit modal close result', () => {
    expect(isWalletModalDismissal({ code: -1, message: 'The user closed the modal.' })).toBe(true);
  });

  it('does not hide real wallet errors', () => {
    expect(isWalletModalDismissal({ code: -1, message: 'Wallet unavailable.' })).toBe(false);
    expect(isWalletModalDismissal(new Error('Connection failed'))).toBe(false);
    expect(isWalletModalDismissal(null)).toBe(false);
  });
});
