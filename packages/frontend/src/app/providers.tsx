'use client';

import { WalletProvider } from '@/lib/wallet-context';

export function Providers({ children }: { children: React.ReactNode }) {
  return <WalletProvider>{children}</WalletProvider>;
}
