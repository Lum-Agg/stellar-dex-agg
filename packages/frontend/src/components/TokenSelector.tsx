'use client';

import { useState, useEffect, useMemo } from 'react';
import { createPortal } from 'react-dom';
import { displayTokenSymbol, NATIVE_CONTRACT } from '@/lib/tokenDisplay';
import { useAccountBalances } from '@/lib/account-balances-context';
import { formatBalanceDisplay } from '@/lib/balance';

export interface Token {
  id: string;
  symbol: string;
  name: string;
  decimals: number;
  color: string;
  logo?: string;
}

const API_URL = process.env.NEXT_PUBLIC_API_URL || 'https://api.lumagg.xyz';

// Colors for tokens based on first char
const TOKEN_COLORS: Record<string, string> = {
  X: '#14B8A6',
  U: '#2775CA',
  E: '#2B6CB0',
  A: '#06B6D4',
  y: '#8B5CF6',
  B: '#F7931A',
  F: '#EC4899',
  S: '#10B981',
  P: '#F59E0B',
  D: '#EF4444',
  C: '#6366F1',
  L: '#84CC16',
};

function getColor(symbol: string): string {
  return TOKEN_COLORS[symbol[0]] || '#6B7280';
}

// Well-known tokens (always shown at top). Logos come from API `/api/v1/tokens`.
const PRIORITY_TOKENS: Token[] = [
  {
    id: 'CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA',
    symbol: 'XLM',
    name: 'Stellar Lumens',
    decimals: 7,
    color: '#14B8A6',
  },
  {
    id: 'CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75',
    symbol: 'USDC',
    name: 'USD Coin',
    decimals: 7,
    color: '#2775CA',
  },
  {
    id: 'CDTKPWPLOURQA2SGTKTUQOWRCBZEORB4BWBOMJ3D3ZTQQSGE5F6JBQLV',
    symbol: 'EURC',
    name: 'Euro Coin',
    decimals: 7,
    color: '#2B6CB0',
  },
  {
    id: 'CAUIKL3IYGMERDRUN6YSCLWVAKIFG5Q4YJHUKM4S4NJZQIA3BAS6OJPK',
    symbol: 'AQUA',
    name: 'Aquarius',
    decimals: 7,
    color: '#06B6D4',
  },
];

// Export for SwapCard default
export const TOKENS: Token[] = PRIORITY_TOKENS;

export function useTokenList() {
  const [tokens, setTokens] = useState<Token[]>(PRIORITY_TOKENS);
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    if (loaded) return;
    fetch(`${API_URL}/api/v1/tokens`)
      .then((r) => r.json())
      .then((data) => {
        if (data.tokens) {
          // Only show tokens that have a real name (not "Unknown")
          const apiTokens: Token[] = data.tokens
            .filter((t: any) => t.name !== 'Unknown')
            .map((t: any) => ({
              id: t.id,
              symbol: displayTokenSymbol(t.symbol, t.id),
              name: t.name,
              decimals: 7,
              color: getColor(t.symbol),
              logo: typeof t.logo === 'string' && t.logo.length > 0 ? t.logo : undefined,
            }));
          const byId = new Map(apiTokens.map((t) => [t.id, t]));
          const priorityIds = new Set(PRIORITY_TOKENS.map((t) => t.id));
          // Merge API logos/names into priority rows (API used to exclude these ids entirely).
          const mergedPriority = PRIORITY_TOKENS.map((p) => {
            const api = byId.get(p.id);
            const logo = api?.logo;
            const symbol = displayTokenSymbol(api?.symbol ?? p.symbol, p.id);
            return {
              ...p,
              symbol,
              name: api?.name ?? p.name,
              color: getColor(symbol),
              logo,
            };
          });
          const others = apiTokens.filter((t) => !priorityIds.has(t.id));
          setTokens([...mergedPriority, ...others]);
        }
        setLoaded(true);
      })
      .catch(() => setLoaded(true));
  }, [loaded]);

  return tokens;
}

export function TokenIcon({ token, size = 28 }: { token: Token; size?: number }) {
  const [imgError, setImgError] = useState(false);
  useEffect(() => {
    setImgError(false);
  }, [token.logo, token.id]);

  if (token.logo && !imgError) {
    return (
      <img
        src={token.logo}
        alt={token.symbol}
        className="rounded-full ring-1 ring-white/10"
        style={{ width: size, height: size }}
        onError={() => setImgError(true)}
      />
    );
  }
  return (
    <div
      className="rounded-full flex items-center justify-center text-white font-bold"
      style={{
        width: size,
        height: size,
        backgroundColor: token.color,
        fontSize: size * 0.4,
      }}
    >
      {token.symbol[0]}
    </div>
  );
}

export function TokenSelector({
  selected,
  onSelect,
  exclude,
  tokens: tokensOverride,
}: {
  selected: Token;
  onSelect: (token: Token) => void;
  exclude?: string;
  /** When set, skip mainnet token list (e.g. testnet Limit panel). */
  tokens?: Token[];
}) {
  const [open, setOpen] = useState(false);
  const [search, setSearch] = useState('');
  const mainnetTokens = useTokenList();
  const tokens = tokensOverride ?? mainnetTokens;
  const { getBalance, ready: balancesReady } = useAccountBalances();

  const q = search.trim();
  const qLower = q.toLowerCase();

  const filtered = useMemo(() => {
    const matched = tokens.filter((t) => {
      if (t.id === exclude) return false;
      const matchesBasic =
        t.symbol.toLowerCase().includes(qLower) ||
        t.name.toLowerCase().includes(qLower) ||
        t.id.toLowerCase().includes(qLower);
      const matchesNative =
        qLower === 'native' && (t.symbol.toLowerCase() === 'xlm' || t.id === NATIVE_CONTRACT);
      return matchesBasic || matchesNative;
    });

    // Non-zero balances first (desc), then catalog order.
    if (!balancesReady) return matched;
    return [...matched].sort((a, b) => {
      const ba = getBalance(a.id) ?? BigInt(0);
      const bb = getBalance(b.id) ?? BigInt(0);
      const aOwned = ba > BigInt(0);
      const bOwned = bb > BigInt(0);
      if (aOwned && bOwned) {
        if (ba === bb) return 0;
        return ba > bb ? -1 : 1;
      }
      if (aOwned !== bOwned) return aOwned ? -1 : 1;
      return 0;
    });
  }, [tokens, exclude, qLower, balancesReady, getBalance]);

  const exactIdMatch =
    filtered.length === 1 && q.length > 0 && qLower === filtered[0].id.toLowerCase();

  const selectExactMatch = () => {
    onSelect(filtered[0]);
    setOpen(false);
    setSearch('');
  };

  return (
    <div className="relative">
      <button
        onClick={() => setOpen(!open)}
        className="flex items-center gap-2 bg-[var(--surface-raised)] hover:bg-[var(--bg-0)] border border-[var(--border)] rounded-xl px-3.5 py-2.5 transition-colors"
      >
        <TokenIcon token={selected} size={24} />
        <span className="font-medium text-[15px] text-[var(--text-primary)]">
          {selected.symbol}
        </span>
        <svg
          className="w-3.5 h-3.5 text-[var(--text-muted)]"
          fill="none"
          viewBox="0 0 24 24"
          stroke="currentColor"
        >
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 9l-7 7-7-7" />
        </svg>
      </button>

      {open &&
        typeof window !== 'undefined' &&
        createPortal(
          <div
            className="fixed inset-0 z-[200] flex items-center justify-center bg-black/75 backdrop-blur-[2px]"
            onClick={() => {
              setOpen(false);
              setSearch('');
            }}
          >
            <div
              className="bg-[var(--surface)] border border-[var(--border)] rounded-2xl w-full max-w-md mx-4 overflow-hidden"
              onClick={(e) => e.stopPropagation()}
            >
              <div className="flex items-center justify-between px-5 py-4 border-b border-[var(--border)]">
                <h3 className="text-[15px] font-semibold text-[var(--text-primary)]">
                  Select a token
                </h3>
                <button
                  onClick={() => {
                    setOpen(false);
                    setSearch('');
                  }}
                  className="text-[var(--text-muted)] hover:text-[var(--text-primary)]"
                >
                  <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path
                      strokeLinecap="round"
                      strokeLinejoin="round"
                      strokeWidth={2}
                      d="M6 18L18 6M6 6l12 12"
                    />
                  </svg>
                </button>
              </div>

              {/* Search */}
              <div className="px-5 py-3">
                <input
                  type="text"
                  value={search}
                  onChange={(e) => setSearch(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter' && exactIdMatch) {
                      e.preventDefault();
                      selectExactMatch();
                    }
                  }}
                  placeholder="Search name, C…, or CODE:ISSUER"
                  className="w-full bg-[var(--bg-0)] border border-[var(--border)] rounded-xl px-4 py-3 text-[13px] outline-none focus:border-[var(--accent)]/40 placeholder-[var(--text-muted)] text-[var(--text-primary)]"
                  autoFocus
                />
              </div>

              {exactIdMatch && (
                <div className="px-5 pb-2">
                  <button
                    type="button"
                    onClick={selectExactMatch}
                    className="w-full flex items-center justify-center gap-2 bg-[var(--accent)]/10 hover:bg-[var(--accent)]/15 border border-[var(--accent)]/30 rounded-xl px-4 py-2.5 text-sm font-medium text-[var(--accent)] transition-colors"
                  >
                    Select {filtered[0].symbol}
                  </button>
                </div>
              )}

              {/* Token list */}
              <div className="max-h-[400px] overflow-y-auto px-2 pb-4">
                {filtered.slice(0, 50).map((token) => {
                  const bal = balancesReady ? getBalance(token.id) : null;
                  return (
                    <button
                      key={token.id}
                      onClick={() => {
                        onSelect(token);
                        setOpen(false);
                        setSearch('');
                      }}
                      className={`w-full flex items-center gap-3 px-4 py-3 rounded-xl hover:bg-white/[0.03] transition-colors ${
                        token.id === selected.id
                          ? 'bg-white/[0.03] border border-[var(--border)]'
                          : ''
                      }`}
                    >
                      <TokenIcon token={token} size={36} />
                      <div className="text-left min-w-0 flex-1">
                        <div className="text-sm font-semibold text-[var(--text-primary)]">
                          {token.symbol}
                        </div>
                        <div className="text-xs text-[var(--text-muted)] truncate">
                          {token.name}
                        </div>
                      </div>
                      {bal !== null && bal > BigInt(0) && (
                        <div className="text-xs text-[var(--text-secondary)] tabular-nums shrink-0 font-[family-name:var(--font-mono)]">
                          {formatBalanceDisplay(bal, token.decimals)}
                        </div>
                      )}
                      {token.id === selected.id && (
                        <svg
                          className="w-4 h-4 text-[var(--accent)] shrink-0"
                          fill="currentColor"
                          viewBox="0 0 20 20"
                        >
                          <path
                            fillRule="evenodd"
                            d="M16.707 5.293a1 1 0 010 1.414l-8 8a1 1 0 01-1.414 0l-4-4a1 1 0 011.414-1.414L8 12.586l7.293-7.293a1 1 0 011.414 0z"
                            clipRule="evenodd"
                          />
                        </svg>
                      )}
                    </button>
                  );
                })}
                {filtered.length > 50 && (
                  <div className="px-4 py-3 text-xs text-[var(--text-muted)] text-center">
                    {filtered.length - 50} more tokens — type to search
                  </div>
                )}
                {filtered.length === 0 && q.length > 0 && (
                  <div className="px-4 py-6 text-center">
                    <span className="text-[var(--text-muted)] text-sm">
                      {q.length >= 4 ? 'Token not in list' : 'No tokens found'}
                    </span>
                  </div>
                )}
              </div>
            </div>
          </div>,
          document.body,
        )}
    </div>
  );
}
