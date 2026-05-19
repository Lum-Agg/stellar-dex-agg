import type { Metadata } from 'next';
import './globals.css';
import { Providers } from './providers';
import { HeaderWallet } from '@/components/HeaderWallet';

export const metadata: Metadata = {
  title: 'LumAgg - Stellar DEX Aggregator',
  description: 'Best swap rates across Stellar DEXes. Split orders for optimal execution.',
};

export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html lang="en" className="dark">
      <body className="bg-[#0a0b0f] text-white min-h-screen antialiased">
        <Providers>
          {/* Background gradient */}
          <div className="fixed inset-0 bg-gradient-to-br from-blue-950/20 via-transparent to-purple-950/20 pointer-events-none" />

          <header className="relative border-b border-white/5 backdrop-blur-sm">
            <div className="max-w-5xl mx-auto px-6 py-4 flex items-center justify-between">
              <a href="/" className="flex items-center gap-2">
                <div className="w-8 h-8 rounded-lg bg-gradient-to-br from-blue-500 to-purple-600 flex items-center justify-center text-sm font-bold">
                  L
                </div>
                <span className="text-lg font-semibold tracking-tight">
                  Lum<span className="text-blue-400">Agg</span>
                </span>
              </a>
              <div className="flex items-center gap-4">
                <nav className="hidden sm:flex items-center gap-6 text-sm text-gray-400">
                  <a href="/" className="hover:text-white transition-colors">
                    Swap
                  </a>
                  <a href="/docs" className="hover:text-white transition-colors">
                    API Docs
                  </a>
                </nav>
                <HeaderWallet />
              </div>
            </div>
          </header>

          <main className="relative max-w-5xl mx-auto px-6 py-12">
            {children}
          </main>

          <footer className="relative border-t border-white/5 mt-20">
            <div className="max-w-5xl mx-auto px-6 py-6 text-center text-xs text-gray-500">
              Aggregating liquidity across Aquarius, Phoenix, Soroswap, Sushi V3, Comet & Stellar Classic DEX
            </div>
          </footer>
        </Providers>
      </body>
    </html>
  );
}
