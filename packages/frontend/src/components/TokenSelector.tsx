'use client';

import { useState, useEffect } from 'react';
import { createPortal } from 'react-dom';

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
  X: '#14B8A6', U: '#2775CA', E: '#2B6CB0', A: '#06B6D4',
  y: '#8B5CF6', B: '#F7931A', F: '#EC4899', S: '#10B981',
  P: '#F59E0B', D: '#EF4444', C: '#6366F1', L: '#84CC16',
};

function getColor(symbol: string): string {
  return TOKEN_COLORS[symbol[0]] || '#6B7280';
}

// Well-known tokens (always shown at top) with logo URLs
const PRIORITY_TOKENS: Token[] = [
  { id: 'CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA', symbol: 'XLM', name: 'Stellar Lumens', decimals: 7, color: '#14B8A6' },
  { id: 'CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75', symbol: 'USDC', name: 'USD Coin', decimals: 7, color: '#2775CA' },
  { id: 'CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC', symbol: 'EURC', name: 'Euro Coin', decimals: 7, color: '#2B6CB0' },
  { id: 'CDTKPWPLOURQA2SGTKTUQOWRCBZEORB4BWBOMJ3D3ZTQQSGE5F6JBQLV', symbol: 'AQUA', name: 'Aquarius', decimals: 7, color: '#06B6D4' },
];

// Export for SwapCard default
export const TOKENS: Token[] = PRIORITY_TOKENS;

export function useTokenList() {
  const [tokens, setTokens] = useState<Token[]>(PRIORITY_TOKENS);
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    if (loaded) return;
    fetch(`${API_URL}/api/v1/tokens`)
      .then(r => r.json())
      .then(data => {
        if (data.tokens) {
          // Only show tokens that have a real name (not "Unknown")
          const apiTokens: Token[] = data.tokens
            .filter((t: any) => t.name !== 'Unknown')
            .map((t: any) => ({
              id: t.id,
              symbol: t.symbol,
              name: t.name,
              decimals: 7,
              color: getColor(t.symbol),
              logo: t.logo,
            }));
          const priorityIds = new Set(PRIORITY_TOKENS.map(t => t.id));
          const others = apiTokens.filter(t => !priorityIds.has(t.id));
          setTokens([...PRIORITY_TOKENS, ...others]);
        }
        setLoaded(true);
      })
      .catch(() => setLoaded(true));
  }, [loaded]);

  return tokens;
}

function TokenIcon({ token, size = 28 }: { token: Token; size?: number }) {
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
}: {
  selected: Token;
  onSelect: (token: Token) => void;
  exclude?: string;
}) {
  const [open, setOpen] = useState(false);
  const [search, setSearch] = useState('');
  const tokens = useTokenList();

  const filtered = tokens.filter(t =>
    t.id !== exclude &&
    (t.symbol.toLowerCase().includes(search.toLowerCase()) ||
     t.name.toLowerCase().includes(search.toLowerCase()) ||
     t.id.toLowerCase().includes(search.toLowerCase()))
  );

  return (
    <div className="relative">
      <button
        onClick={() => setOpen(!open)}
        className="flex items-center gap-2 bg-slate-800/70 hover:bg-slate-800 border border-white/15 rounded-xl px-3 py-2 transition-colors"
      >
        <TokenIcon token={selected} size={22} />
        <span className="font-medium text-sm">{selected.symbol}</span>
        <svg className="w-3 h-3 text-gray-400" fill="none" viewBox="0 0 24 24" stroke="currentColor">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 9l-7 7-7-7" />
        </svg>
      </button>

      {open && typeof window !== 'undefined' && createPortal(
        <div className="fixed inset-0 z-[200] flex items-center justify-center bg-black/60 backdrop-blur-sm" onClick={() => { setOpen(false); setSearch(''); }}>
          <div className="bg-slate-900 border border-white/15 rounded-2xl shadow-2xl w-full max-w-md mx-4 overflow-hidden" onClick={(e) => e.stopPropagation()}>
            {/* Header */}
            <div className="flex items-center justify-between px-5 py-4 border-b border-white/10">
              <h3 className="text-base font-semibold">Select a token</h3>
              <button onClick={() => { setOpen(false); setSearch(''); }} className="text-slate-400 hover:text-white">
                <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                </svg>
              </button>
            </div>

            {/* Search */}
            <div className="px-5 py-3">
              <input
                type="text"
                value={search}
                onChange={(e) => setSearch(e.target.value)}
                placeholder="Search name or paste address"
                className="w-full bg-slate-950/60 border border-white/10 rounded-xl px-4 py-3 text-sm outline-none focus:border-blue-500/50 placeholder-slate-500"
                autoFocus
              />
            </div>

            {/* Token list */}
            <div className="max-h-[400px] overflow-y-auto px-2 pb-4">
              {filtered.slice(0, 50).map(token => (
                <button
                  key={token.id}
                  onClick={() => { onSelect(token); setOpen(false); setSearch(''); }}
                  className={`w-full flex items-center gap-3 px-4 py-3 rounded-xl hover:bg-white/5 transition-colors ${
                    token.id === selected.id ? 'bg-blue-500/10 border border-blue-400/30' : ''
                  }`}
                >
                  <TokenIcon token={token} size={36} />
                  <div className="text-left min-w-0 flex-1">
                    <div className="text-sm font-semibold">{token.symbol}</div>
                    <div className="text-xs text-slate-500 truncate">{token.name}</div>
                  </div>
                  {token.id === selected.id && (
                    <svg className="w-4 h-4 text-blue-400" fill="currentColor" viewBox="0 0 20 20">
                      <path fillRule="evenodd" d="M16.707 5.293a1 1 0 010 1.414l-8 8a1 1 0 01-1.414 0l-4-4a1 1 0 011.414-1.414L8 12.586l7.293-7.293a1 1 0 011.414 0z" clipRule="evenodd" />
                    </svg>
                  )}
                </button>
              ))}
              {filtered.length > 50 && (
                <div className="px-4 py-3 text-xs text-slate-500 text-center">
                  {filtered.length - 50} more tokens — type to search
                </div>
              )}
              {filtered.length === 0 && (
                <div className="px-4 py-6 text-center">
                  {search.startsWith('C') && search.length > 40 ? (
                    <button
                      onClick={() => {
                        onSelect({ id: search, symbol: search.slice(0, 6), name: 'Custom Token', decimals: 7, color: '#6B7280' });
                        setOpen(false);
                        setSearch('');
                      }}
                      className="text-blue-400 hover:text-blue-300 text-sm"
                    >
                      Use {search.slice(0, 12)}... as custom token
                    </button>
                  ) : (
                    <span className="text-slate-500 text-sm">No tokens found</span>
                  )}
                </div>
              )}
            </div>
          </div>
        </div>,
        document.body
      )}
    </div>
  );
}
