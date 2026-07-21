/**
 * Build and submit classic ChangeTrust for SAC-backed assets (frontend-only).
 */

import { Account, Asset, BASE_FEE, Networks, Operation, TransactionBuilder } from '@stellar/stellar-sdk';
import { NATIVE_CONTRACT } from '@/lib/tokenDisplay';
import { fetchAccountSequence } from '@/lib/rpc';

const NETWORK_PASSPHRASE = Networks.PUBLIC;

export interface ClassicAssetRef {
  code: string;
  issuer: string;
}

/** SAC contract id → classic asset (same mapping as dex-adapters CLASSIC_ASSETS). */
const CLASSIC_ASSET_BY_SAC: Record<string, ClassicAssetRef> = {
  CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75: {
    code: 'USDC',
    issuer: 'GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN',
  },
  CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC: {
    code: 'EURC',
    issuer: 'GDHU6WRG4IEQXM5NZ4BMPKOXHW76MZM4Y2IEMFDVXBSDP6SJY4ITNPP2',
  },
};

export function classicAssetForSac(contractId: string): ClassicAssetRef | null {
  if (contractId === NATIVE_CONTRACT || contractId === 'native') return null;
  return CLASSIC_ASSET_BY_SAC[contractId] ?? null;
}

export function canAddTrustlineForSac(contractId: string): boolean {
  return classicAssetForSac(contractId) !== null;
}

/** Unsigned ChangeTrust XDR (max limit — wallet default). */
export async function buildChangeTrustXdr(
  accountId: string,
  asset: ClassicAssetRef,
): Promise<string> {
  const sequence = await fetchAccountSequence(accountId);
  const stellarAsset = new Asset(asset.code, asset.issuer);
  const source = new Account(accountId, sequence);
  const tx = new TransactionBuilder(source, {
    fee: BASE_FEE,
    networkPassphrase: NETWORK_PASSPHRASE,
  })
    .addOperation(Operation.changeTrust({ asset: stellarAsset }))
    .setTimeout(300)
    .build();
  return tx.toXDR();
}
