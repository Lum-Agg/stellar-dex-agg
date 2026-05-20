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
      <body className="min-h-screen antialiased text-slate-100 flex flex-col">
        <Providers>
          <div className="fixed inset-0 pointer-events-none bg-[linear-gradient(to_bottom,rgba(148,163,184,0.06)_1px,transparent_1px)] bg-[size:100%_24px] opacity-20" />

          <header className="sticky top-0 z-40 shrink-0 border-b border-white/10 bg-[#0a0f1be6] backdrop-blur-xl">
            <div className="max-w-5xl mx-auto px-6 py-4 flex items-center justify-between">
              <a href="/" className="flex items-center gap-2.5">
                <div className="w-8 h-8 rounded-lg bg-gradient-to-br from-blue-500 to-violet-500 flex items-center justify-center text-sm font-bold shadow-lg shadow-blue-500/20">
                  L
                </div>
                <span className="text-lg font-semibold tracking-tight text-slate-100">
                  Lum<span className="text-blue-400">Agg</span>
                </span>
              </a>
              <div className="flex items-center gap-4">
                <nav className="hidden sm:flex items-center gap-6 text-sm text-slate-400">
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

          <main className="relative flex-1 flex flex-col max-w-5xl w-full mx-auto px-6 py-10 md:py-14">
            {children}
          </main>

          <footer className="relative shrink-0 border-t border-white/10 mt-auto bg-[#0a0f1b80]">
            <div className="max-w-5xl mx-auto px-6 py-8 text-center text-xs text-slate-500">
              Mainnet routing across Aquarius, Phoenix, Soroswap, Sushi V3, Comet and Stellar Classic DEX.
            </div>
          </footer>
        </Providers>
      </body>
    </html>
  );
}
