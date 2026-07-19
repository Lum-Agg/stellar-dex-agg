import type { Metadata } from 'next';
import { Inter } from 'next/font/google';
import './globals.css';
import { Providers } from './providers';
import { HeaderWallet } from '@/components/HeaderWallet';
import { GITHUB_REPO_URL } from '@/lib/site';

const inter = Inter({
  subsets: ['latin'],
  display: 'swap',
});

export const metadata: Metadata = {
  title: 'LumAgg - Stellar DEX Aggregator',
  description: 'Best swap rates across Stellar DEXes. Split orders for optimal execution.',
  icons: {
    icon: '/favicon.ico',
    shortcut: '/favicon.ico',
  },
};

export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html lang="en" className="dark" suppressHydrationWarning>
      <body className={`${inter.className} min-h-screen antialiased text-zinc-100`}>
        <Providers>
          <header className="sticky top-0 z-40 shrink-0 border-b border-white/[0.06] bg-[#09090b]/80 backdrop-blur-md">
            <div className="max-w-5xl mx-auto px-5 sm:px-6 h-14 flex items-center justify-between">
              <a href="/" className="flex items-center gap-2.5 group">
                <div
                  className="w-8 h-8 rounded-lg bg-gradient-to-br from-blue-500 to-indigo-600 flex items-center justify-center text-sm font-bold text-white shadow-md shadow-blue-500/20"
                  aria-hidden
                >
                  L
                </div>
                <span className="text-[15px] font-semibold tracking-tight text-zinc-100">
                  Lum<span className="text-blue-400">Agg</span>
                </span>
              </a>
              <div className="flex items-center gap-3 sm:gap-5">
                <nav className="hidden sm:flex items-center gap-5 text-[13px] text-zinc-400">
                  <a href="/" className="hover:text-zinc-100 transition-colors">
                    Swap
                  </a>
                  <a href="/portfolio" className="hover:text-zinc-100 transition-colors">
                    Portfolio
                  </a>
                  <a href="/docs" className="hover:text-zinc-100 transition-colors">
                    API Docs
                  </a>
                  <a href="/stats" className="hover:text-zinc-100 transition-colors">
                    Stats
                  </a>
                  <a
                    href={GITHUB_REPO_URL}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="inline-flex items-center gap-1.5 hover:text-zinc-100 transition-colors"
                  >
                    <GitHubIcon className="w-3.5 h-3.5" />
                    GitHub
                  </a>
                </nav>
                <a
                  href={GITHUB_REPO_URL}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="sm:hidden inline-flex items-center justify-center w-8 h-8 rounded-md border border-white/[0.08] text-zinc-400 hover:text-zinc-100 transition-colors"
                  aria-label="GitHub repository"
                >
                  <GitHubIcon className="w-3.5 h-3.5" />
                </a>
                <HeaderWallet />
              </div>
            </div>
          </header>

          <main className="relative max-w-5xl w-full mx-auto px-5 sm:px-6 py-10 md:py-12 min-w-0">
            {children}
          </main>

          <footer className="relative border-t border-white/[0.06] mt-16">
            <div className="max-w-5xl mx-auto px-5 sm:px-6 py-8 flex flex-col sm:flex-row items-start sm:items-center justify-between gap-3 text-[13px] text-zinc-500">
              <span className="max-w-lg leading-relaxed">
                Mainnet routing across Aquarius, Phoenix, Soroswap, Sushi V3, Comet and Stellar Classic DEX.
              </span>
              <a
                href={GITHUB_REPO_URL}
                target="_blank"
                rel="noopener noreferrer"
                className="inline-flex items-center gap-1.5 text-zinc-400 hover:text-zinc-200 transition-colors shrink-0"
              >
                <GitHubIcon className="w-3.5 h-3.5" />
                Open source
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
