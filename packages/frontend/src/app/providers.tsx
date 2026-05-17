'use client';

import dynamic from 'next/dynamic';

// Dynamic import with SSR disabled — stellar-wallets-kit uses browser APIs
const WalletProviderInner = dynamic(
  () => import('@/lib/wallet-context').then(mod => ({ default: mod.WalletProvider })),
  { ssr: false }
);

export function Providers({ children }: { children: React.ReactNode }) {
  return <WalletProviderInner>{children}</WalletProviderInner>;
}
