/**
 * Build and submit classic ChangeTrust for SAC-backed assets (frontend-only).
 */

import { Account, Asset, BASE_FEE, Networks, Operation, TransactionBuilder } from '@stellar/stellar-sdk';
import { NATIVE_CONTRACT } from '@/lib/tokenDisplay';
import { fetchAccountSequence } from '@/lib/rpc';

const NETWORK_PASSPHRASE = Networks.PUBLIC;
const API_URL = process.env.NEXT_PUBLIC_API_URL || 'https://api.lumagg.xyz';

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
  CDTKPWPLOURQA2SGTKTUQOWRCBZEORB4BWBOMJ3D3ZTQQSGE5F6JBQLV: {
    code: 'EURC',
    issuer: 'GDHU6WRG4IEQXM5NZ4BMPKOXHW76MZM4Y2IEMFDVXBSDP6SJY4ITNPP2',
  },
};

const resolveCache = new Map<string, ClassicAssetRef | null>();

/** Sync lookup for well-known SACs only. Prefer {@link resolveClassicAssetForSac}. */
export function classicAssetForSac(contractId: string): ClassicAssetRef | null {
  if (contractId === NATIVE_CONTRACT || contractId === 'native') return null;
  return CLASSIC_ASSET_BY_SAC[contractId] ?? null;
}

/** True when contract looks like a non-native SAC that might support ChangeTrust. */
export function canAddTrustlineForSac(contractId: string): boolean {
  if (!contractId || contractId === NATIVE_CONTRACT || contractId === 'native') return false;
  return contractId.startsWith('C') && contractId.length === 56;
}

function parseExpertAssetField(asset: string): ClassicAssetRef | null {
  const dash = asset.indexOf('-');
  if (dash <= 0) return null;
  const code = asset.slice(0, dash);
  if (code.length < 1 || code.length > 12 || !/^[A-Za-z0-9]+$/.test(code)) return null;
  const issuer = asset
    .slice(dash + 1)
    .split('-')
    .find((p) => p.length === 56 && p.startsWith('G'));
  if (!issuer) return null;
  return { code, issuer };
}

async function resolveViaApi(contractId: string): Promise<ClassicAssetRef | null> {
  const params = new URLSearchParams({ contract: contractId });
  const resp = await fetch(`${API_URL}/api/v1/classic_asset?${params}`);
  if (!resp.ok) return null;
  const data = (await resp.json()) as {
    success?: boolean;
    code?: string;
    issuer?: string;
  };
  if (!data.success || !data.code || !data.issuer) return null;
  return { code: data.code, issuer: data.issuer };
}

async function resolveViaExpert(contractId: string): Promise<ClassicAssetRef | null> {
  const resp = await fetch(
    `https://api.stellar.expert/explorer/public/contract/${contractId}`,
  );
  if (!resp.ok) return null;
  const data = (await resp.json()) as { asset?: string };
  if (!data.asset) return null;
  return parseExpertAssetField(data.asset);
}

/**
 * Resolve SAC → classic code/issuer for ChangeTrust.
 * Whitelist → LumAgg API → stellar.expert (CORS *).
 */
export async function resolveClassicAssetForSac(
  contractId: string,
): Promise<ClassicAssetRef | null> {
  if (!canAddTrustlineForSac(contractId)) return null;

  const known = classicAssetForSac(contractId);
  if (known) return known;

  if (resolveCache.has(contractId)) {
    return resolveCache.get(contractId) ?? null;
  }

  let resolved: ClassicAssetRef | null = null;
  try {
    resolved = await resolveViaApi(contractId);
  } catch {
    resolved = null;
  }
  if (!resolved) {
    try {
      resolved = await resolveViaExpert(contractId);
    } catch {
      resolved = null;
    }
  }

  resolveCache.set(contractId, resolved);
  return resolved;
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
