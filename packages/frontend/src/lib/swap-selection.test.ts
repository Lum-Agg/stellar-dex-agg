import { describe, expect, it } from 'vitest';
import { resolveTokenSelection, resolveUrlTokenPair } from './swap-selection';

type Token = {
  id: string;
  symbol: string;
};

const XLM: Token = { id: 'xlm', symbol: 'XLM' };
const USDC: Token = { id: 'usdc', symbol: 'USDC' };
const AQUA: Token = { id: 'aqua', symbol: 'AQUA' };

describe('resolveTokenSelection', () => {
  it('swaps both sides when selecting the other side token', () => {
    expect(
      resolveTokenSelection({
        current: XLM,
        other: USDC,
        next: USDC,
      }),
    ).toEqual({
      current: USDC,
      other: XLM,
      swapped: true,
    });
  });

  it('only replaces the active side when selecting a different token', () => {
    expect(
      resolveTokenSelection({
        current: XLM,
        other: USDC,
        next: AQUA,
      }),
    ).toEqual({
      current: AQUA,
      other: USDC,
      swapped: false,
    });
  });
});

describe('resolveUrlTokenPair', () => {
  it('applies a complete requested pair', () => {
    expect(
      resolveUrlTokenPair({
        currentIn: XLM,
        currentOut: USDC,
        requestedIn: AQUA,
        requestedOut: XLM,
        available: [XLM, USDC, AQUA],
      }),
    ).toEqual({ tokenIn: AQUA, tokenOut: XLM, sameTokenRejected: false });
  });

  it('moves the current input token to output when only input would duplicate it', () => {
    expect(
      resolveUrlTokenPair({
        currentIn: XLM,
        currentOut: USDC,
        requestedIn: USDC,
        available: [XLM, USDC, AQUA],
      }),
    ).toEqual({ tokenIn: USDC, tokenOut: XLM, sameTokenRejected: false });
  });

  it('chooses another input when only output would duplicate it', () => {
    expect(
      resolveUrlTokenPair({
        currentIn: XLM,
        currentOut: USDC,
        requestedOut: XLM,
        available: [XLM, USDC, AQUA],
      }),
    ).toEqual({ tokenIn: USDC, tokenOut: XLM, sameTokenRejected: false });
  });

  it('rejects a complete same-token pair and keeps the defaults', () => {
    expect(
      resolveUrlTokenPair({
        currentIn: XLM,
        currentOut: USDC,
        requestedIn: AQUA,
        requestedOut: AQUA,
        available: [XLM, USDC, AQUA],
      }),
    ).toEqual({ tokenIn: XLM, tokenOut: USDC, sameTokenRejected: true });
  });
});
