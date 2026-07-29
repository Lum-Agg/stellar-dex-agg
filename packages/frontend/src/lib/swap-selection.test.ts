import { describe, expect, it } from 'vitest';
import { resolveTokenSelection } from './swap-selection';

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
