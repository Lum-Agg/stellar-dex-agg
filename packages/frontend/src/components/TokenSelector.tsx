'use client';

import { useState } from 'react';

export interface Token {
  id: string;
  symbol: string;
  name: string;
  decimals: number;
  color: string;
}

// Well-known tokens with their SAC contract addresses
export const TOKENS: Token[] = [
  { id: 'CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA', symbol: 'XLM', name: 'Stellar Lumens', decimals: 7, color: '#14B8A6' },
  { id: 'CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75', symbol: 'USDC', name: 'USD Coin', decimals: 7, color: '#2775CA' },
  { id: 'CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC', symbol: 'EURC', name: 'Euro Coin', decimals: 7, color: '#2B6CB0' },
  { id: 'CDTKPWPLOURQA2SGTKTUQOWRCBZEORB4BWBOMJ3D3ZTQQSGE5F6JBQLV', symbol: 'AQUA', name: 'Aquarius', decimals: 7, color: '#06B6D4' },
  { id: 'CBZVSNVB55ANF3QVVZJGD6EBOCTT3BKYZXFHPBHA7DCJZ5CUNFPZRSR3', symbol: 'yXLM', name: 'Yield XLM', decimals: 7, color: '#8B5CF6' },
  { id: 'CAAP2HKDLH7C2GCEGJGKYADET2MUTPBXBFGFYLU7JKDZ7IAFNWPXQ', symbol: 'BTC', name: 'Bitcoin (wrapped)', decimals: 7, color: '#F7931A' },
  { id: 'CAZAQB3D7KSLSNOSQKYD2V4JP5V2Y3B4RDJZRLBFCCIXDCTE3WHSY3UE', symbol: 'ETH', name: 'Ethereum (wrapped)', decimals: 7, color: '#627EEA' },
  { id: 'CCGIMRMF6MFQFGSXORCPUQPJLMCUNZYW5LXNHZGBRT3TYHKV4BALBHP3', symbol: 'FIDR', name: 'Fidr Token', decimals: 7, color: '#EC4899' },
];

function TokenIcon({ token, size = 28 }: { token: Token; size?: number }) {
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

  const filtered = TOKENS.filter(t =>
    t.id !== exclude &&
    (t.symbol.toLowerCase().includes(search.toLowerCase()) ||
     t.name.toLowerCase().includes(search.toLowerCase()))
  );

  return (
    <div className="relative">
      <button
        onClick={() => setOpen(!open)}
        className="flex items-center gap-2 bg-[#252630] hover:bg-[#2a2b38] border border-white/10 rounded-xl px-3 py-2 transition-colors"
      >
        <TokenIcon token={selected} size={22} />
        <span className="font-medium text-sm">{selected.symbol}</span>
        <svg className="w-3 h-3 text-gray-400" fill="none" viewBox="0 0 24 24" stroke="currentColor">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 9l-7 7-7-7" />
        </svg>
      </button>

      {open && (
        <>
          <div className="fixed inset-0 z-40" onClick={() => { setOpen(false); setSearch(''); }} />
          <div className="absolute right-0 top-full mt-2 z-50 bg-[#1a1b23] border border-white/10 rounded-xl shadow-xl overflow-hidden min-w-[200px]">
            {/* Search */}
            <div className="p-2 border-b border-white/5">
              <input
                type="text"
                value={search}
                onChange={(e) => setSearch(e.target.value)}
                placeholder="Search token..."
                className="w-full bg-[#12131a] border border-white/10 rounded-lg px-3 py-1.5 text-xs outline-none focus:border-blue-500/50 placeholder-gray-600"
                autoFocus
              />
            </div>
            {/* Token list */}
            <div className="max-h-[240px] overflow-y-auto">
              {filtered.map(token => (
                <button
                  key={token.id}
                  onClick={() => { onSelect(token); setOpen(false); setSearch(''); }}
                  className={`w-full flex items-center gap-3 px-4 py-2.5 hover:bg-white/5 transition-colors ${
                    token.id === selected.id ? 'bg-white/5' : ''
                  }`}
                >
                  <TokenIcon token={token} size={24} />
                  <div className="text-left">
                    <div className="text-sm font-medium">{token.symbol}</div>
                    <div className="text-[10px] text-gray-500">{token.name}</div>
                  </div>
                </button>
              ))}
              {filtered.length === 0 && (
                <div className="px-4 py-3 text-xs text-gray-500 text-center">No tokens found</div>
              )}
            </div>
          </div>
        </>
      )}
    </div>
  );
}
