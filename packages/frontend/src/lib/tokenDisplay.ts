/** Stellar native asset (XLM) SAC contract on mainnet. */
export const NATIVE_CONTRACT =
  'CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA';

/** User-facing symbol; API/metadata often returns "native" for XLM. */
export function displayTokenSymbol(symbol: string, contractId?: string): string {
  if (
    contractId === NATIVE_CONTRACT ||
    contractId === 'native' ||
    symbol.toLowerCase() === 'native'
  ) {
    return 'XLM';
  }
  return symbol;
}
