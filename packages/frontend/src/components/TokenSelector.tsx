'use client';

import { useState, useEffect, useId, useMemo, useRef, useCallback } from 'react';
import { createPortal } from 'react-dom';
import Image from 'next/image';
import { displayTokenSymbol, NATIVE_CONTRACT } from '@/lib/tokenDisplay';
import { useAccountBalances } from '@/lib/account-balances-context';
import { formatBalanceDisplay } from '@/lib/balance';
import { fetchJson } from '@/lib/fetch-json';

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

type TokenApiResponse = {
  tokens?: Array<{
    id: string;
    symbol: string;
    name: string;
    logo?: string | null;
  }>;
};

let tokenCatalogCache: Token[] | null = null;
let tokenCatalogRequest: Promise<Token[]> | null = null;

function normalizeTokenCatalog(data: TokenApiResponse): Token[] {
  if (!Array.isArray(data.tokens)) return PRIORITY_TOKENS;

  const apiTokens: Token[] = data.tokens
    .filter(
      (token) =>
        token &&
        typeof token.id === 'string' &&
        typeof token.symbol === 'string' &&
        typeof token.name === 'string' &&
        token.name !== 'Unknown',
    )
    .map((token) => ({
      id: token.id,
      symbol: displayTokenSymbol(token.symbol, token.id),
      name: token.name,
      decimals: 7,
      color: getColor(token.symbol),
      logo: typeof token.logo === 'string' && token.logo.length > 0 ? token.logo : undefined,
    }));
  const byId = new Map(apiTokens.map((token) => [token.id, token]));
  const priorityIds = new Set(PRIORITY_TOKENS.map((token) => token.id));
  const mergedPriority = PRIORITY_TOKENS.map((priorityToken) => {
    const apiToken = byId.get(priorityToken.id);
    const symbol = displayTokenSymbol(apiToken?.symbol ?? priorityToken.symbol, priorityToken.id);
    return {
      ...priorityToken,
      symbol,
      name: apiToken?.name ?? priorityToken.name,
      color: getColor(symbol),
      logo: apiToken?.logo,
    };
  });

  return [...mergedPriority, ...apiTokens.filter((token) => !priorityIds.has(token.id))];
}

function loadTokenCatalog(): Promise<Token[]> {
  if (tokenCatalogCache) return Promise.resolve(tokenCatalogCache);
  if (tokenCatalogRequest) return tokenCatalogRequest;

  tokenCatalogRequest = fetchJson<TokenApiResponse>(`${API_URL}/api/v1/tokens`)
    .then((data) => {
      tokenCatalogCache = normalizeTokenCatalog(data);
      return tokenCatalogCache;
    })
    .finally(() => {
      tokenCatalogRequest = null;
    });

  return tokenCatalogRequest;
}

export function useTokenCatalog(enabled = true): { tokens: Token[]; loaded: boolean } {
  const [tokens, setTokens] = useState<Token[]>(() => tokenCatalogCache ?? PRIORITY_TOKENS);
  const [loaded, setLoaded] = useState(() => tokenCatalogCache !== null || !enabled);

  useEffect(() => {
    if (!enabled) {
      setLoaded(true);
      return;
    }

    let cancelled = false;
    void loadTokenCatalog()
      .then((catalog) => {
        if (!cancelled) setTokens(catalog);
      })
      .catch(() => undefined)
      .finally(() => {
        if (!cancelled) setLoaded(true);
      });

    return () => {
      cancelled = true;
    };
  }, [enabled]);

  return { tokens, loaded };
}

export function useTokenList(): Token[] {
  return useTokenCatalog().tokens;
}

export function TokenIcon({ token, size = 28 }: { token: Token; size?: number }) {
  const [imgError, setImgError] = useState(false);
  useEffect(() => {
    setImgError(false);
  }, [token.logo, token.id]);

  if (token.logo && !imgError) {
    return (
      <Image
        src={token.logo}
        alt={token.symbol}
        width={size}
        height={size}
        unoptimized
        className="rounded-full ring-1 ring-white/10"
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
  onSelectExcluded,
  tokens: tokensOverride,
}: {
  selected: Token;
  onSelect: (token: Token) => void;
  exclude?: string;
  onSelectExcluded?: (token: Token) => void;
  /** When set, skip mainnet token list (e.g. testnet Limit panel). */
  tokens?: Token[];
}) {
  const [open, setOpen] = useState(false);
  const [search, setSearch] = useState('');
  const titleId = useId();
  const triggerRef = useRef<HTMLButtonElement>(null);
  const dialogRef = useRef<HTMLDivElement>(null);
  const { tokens: mainnetTokens } = useTokenCatalog(tokensOverride === undefined);
  const tokens = tokensOverride ?? mainnetTokens;
  const { getBalance, ready: balancesReady } = useAccountBalances();

  const q = search.trim();
  const qLower = q.toLowerCase();

  const filtered = useMemo(() => {
    const matched = tokens.filter((t) => {
      if (t.id === exclude && !onSelectExcluded) return false;
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
  }, [tokens, qLower, balancesReady, getBalance, exclude, onSelectExcluded]);

  const closeSelector = useCallback(() => {
    setOpen(false);
    setSearch('');
    queueMicrotask(() => triggerRef.current?.focus());
  }, []);

  useEffect(() => {
    if (!open) return;

    const previousOverflow = document.body.style.overflow;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        closeSelector();
        return;
      }
      if (event.key !== 'Tab') return;

      const focusable = dialogRef.current?.querySelectorAll<HTMLElement>(
        'button:not([disabled]), input:not([disabled]), [href], [tabindex]:not([tabindex="-1"])',
      );
      if (!focusable?.length) return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };

    document.body.style.overflow = 'hidden';
    document.addEventListener('keydown', onKeyDown);
    return () => {
      document.body.style.overflow = previousOverflow;
      document.removeEventListener('keydown', onKeyDown);
    };
  }, [closeSelector, open]);

  const exactIdMatch =
    filtered.length === 1 && q.length > 0 && qLower === filtered[0].id.toLowerCase();

  const selectExactMatch = () => {
    onSelect(filtered[0]);
    closeSelector();
  };

  return (
    <div className="relative">
      <button
        ref={triggerRef}
        type="button"
        onClick={() => (open ? closeSelector() : setOpen(true))}
        className="flex items-center gap-1.5 sm:gap-2 bg-[var(--surface-raised)] hover:bg-[var(--bg-0)] border border-[var(--border)] rounded-xl px-2.5 sm:px-3.5 py-2.5 transition-colors"
        aria-haspopup="dialog"
        aria-expanded={open}
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
            role="presentation"
            onClick={closeSelector}
          >
            <div
              ref={dialogRef}
              className="flex max-h-[calc(100dvh-2rem)] w-full max-w-md flex-col overflow-hidden rounded-2xl border border-[var(--border)] bg-[var(--surface)] mx-4"
              role="dialog"
              aria-modal="true"
              aria-labelledby={titleId}
              onClick={(e) => e.stopPropagation()}
            >
              <div className="flex items-center justify-between px-5 py-4 border-b border-[var(--border)]">
                <h3 id={titleId} className="text-[15px] font-semibold text-[var(--text-primary)]">
                  Select a token
                </h3>
                <button
                  type="button"
                  onClick={closeSelector}
                  className="text-[var(--text-muted)] hover:text-[var(--text-primary)]"
                  aria-label="Close token selector"
                >
                  <svg
                    className="w-5 h-5"
                    fill="none"
                    viewBox="0 0 24 24"
                    stroke="currentColor"
                    aria-hidden
                  >
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
              <div className="min-h-0 max-h-[400px] flex-1 overflow-y-auto px-2 pb-4">
                {filtered.slice(0, 50).map((token) => {
                  const bal = balancesReady ? getBalance(token.id) : null;
                  const isExcluded = token.id === exclude;
                  return (
                    <button
                      key={token.id}
                      type="button"
                      onClick={() => {
                        if (isExcluded && onSelectExcluded) {
                          onSelectExcluded(token);
                        } else {
                          onSelect(token);
                        }
                        closeSelector();
                      }}
                      className={`w-full flex items-center gap-3 px-4 py-3 rounded-xl transition-colors ${
                        token.id === selected.id
                          ? 'bg-white/[0.03] border border-[var(--border)]'
                          : 'hover:bg-white/[0.03]'
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
