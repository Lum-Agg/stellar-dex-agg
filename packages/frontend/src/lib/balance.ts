/** Horizon balance lookup for pre-swap checks (stroops). */

const HORIZON = 'https://horizon.stellar.org';
const NATIVE_SAC = 'CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA';

function horizonBalanceToStroops(balance: string, decimals: number): bigint {
  const parts = balance.split('.');
  const whole = parts[0] || '0';
  const frac = (parts[1] ?? '').padEnd(decimals, '0').slice(0, decimals);
  return BigInt(whole) * BigInt(10 ** decimals) + BigInt(frac || '0');
}

export async function fetchSpendableBalanceStroops(
  accountId: string,
  tokenContractId: string,
  decimals: number
): Promise<bigint | null> {
  try {
    const resp = await fetch(`${HORIZON}/accounts/${accountId}`);
    if (!resp.ok) return null;
    const data = (await resp.json()) as {
      balances?: Array<{
        balance?: string;
        asset_type?: string;
        asset_code?: string;
        asset_issuer?: string;
        liquidity_pool_id?: string;
      }>;
    };

    for (const b of data.balances ?? []) {
      if (tokenContractId === NATIVE_SAC && b.asset_type === 'native' && b.balance) {
        return horizonBalanceToStroops(b.balance, decimals);
      }
      // Soroban SAC / contract balances may appear as credit entries on some accounts
      if (
        b.asset_type === 'credit_alphanum4' &&
        b.asset_code &&
        b.balance &&
        !b.liquidity_pool_id
      ) {
        // Matched only when caller maps code:issuer → contract elsewhere
        continue;
      }
    }
    return null;
  } catch {
    return null;
  }
}
