import type { Metadata } from 'next';
import './globals.css';
import { Providers } from './providers';
import { HeaderWallet } from '@/components/HeaderWallet';
import { GITHUB_REPO_URL } from '@/lib/site';

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
      <body className="min-h-screen antialiased text-slate-100">
        <Providers>
          <div className="fixed inset-0 pointer-events-none bg-[linear-gradient(to_bottom,rgba(148,163,184,0.06)_1px,transparent_1px)] bg-[size:100%_24px] opacity-20" />

          <header className="sticky top-0 z-40 shrink-0 border-b border-white/10 bg-[#0a0f1be6] backdrop-blur-xl">
            <div className="max-w-4xl mx-auto px-6 py-4 flex items-center justify-between">
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
                  <a
                    href={GITHUB_REPO_URL}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="inline-flex items-center gap-1.5 hover:text-white transition-colors"
                  >
                    <GitHubIcon className="w-4 h-4" />
                    GitHub
                  </a>
                </nav>
                <a
                  href={GITHUB_REPO_URL}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="sm:hidden inline-flex items-center justify-center w-9 h-9 rounded-lg border border-white/10 text-slate-400 hover:text-white hover:border-white/20 transition-colors"
                  aria-label="GitHub repository"
                >
                  <GitHubIcon className="w-4 h-4" />
                </a>
                <HeaderWallet />
              </div>
            </div>
          </header>

          <main className="relative max-w-4xl w-full mx-auto px-6 py-8 md:py-10 min-w-0">
            {children}
          </main>

          <footer className="relative border-t border-white/10 mt-12 bg-[#0a0f1b80]">
            <div className="max-w-4xl mx-auto px-6 py-6 flex flex-col sm:flex-row items-center justify-center gap-2 sm:gap-4 text-xs text-slate-500">
              <span className="text-center">
                Mainnet routing across Aquarius, Phoenix, Soroswap, Sushi V3, Comet and Stellar Classic DEX.
              </span>
              <span className="hidden sm:inline text-slate-600">·</span>
              <a
                href={GITHUB_REPO_URL}
                target="_blank"
                rel="noopener noreferrer"
                className="inline-flex items-center gap-1.5 text-slate-400 hover:text-slate-200 transition-colors"
              >
                <GitHubIcon className="w-3.5 h-3.5" />
                Open source on GitHub
              </a>
            </div>
          </footer>
        </Providers>
      </body>
    </html>
  );
}

function GitHubIcon({ className }: { className?: string }) {
  return (
    <svg
      className={className}
      viewBox="0 0 24 24"
      fill="currentColor"
      aria-hidden
    >
      <path d="M12 0C5.37 0 0 5.37 0 12c0 5.31 3.435 9.795 8.205 11.385.6.105.825-.255.825-.57 0-.285-.015-1.04-.015-2.04-3.338.724-4.042-1.61-4.042-1.61-.546-1.385-1.335-1.755-1.335-1.755-1.087-.744.084-.729.084-.729 1.205.084 1.838 1.236 1.838 1.236 1.07 1.835 2.809 1.305 3.495.998.108-.776.417-1.305.76-1.605-2.665-.3-5.466-1.332-5.466-5.93 0-1.31.465-2.38 1.235-3.22-.135-.3-.54-1.52.105-3.17 0 0 1.005-.322 3.3 1.23.96-.27 1.98-.405 3-.405 1.02 0 2.04.135 3 .405 2.28-1.552 3.285-1.23 3.285-1.23.645 1.65.24 2.87.12 3.17.765.84 1.23 1.91 1.23 3.22 0 4.61-2.805 5.625-5.475 5.92.42.36.81 1.096.81 2.22 0 1.605-.015 2.896-.015 3.286 0 .315.21.69.825.57A12.02 12.02 0 0 0 24 12c0-6.63-5.37-12-12-12z" />
    </svg>
  );
}
