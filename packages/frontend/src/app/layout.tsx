import type { Metadata } from 'next';
import Image from 'next/image';
import Link from 'next/link';
import { DM_Sans, JetBrains_Mono } from 'next/font/google';
import './globals.css';
import { Providers } from './providers';
import { HeaderWallet } from '@/components/HeaderWallet';
import { HeaderNav } from '@/components/HeaderNav';
import { DOCUMENTATION_URL, GITHUB_REPO_URL } from '@/lib/site';

const dmSans = DM_Sans({
  subsets: ['latin'],
  display: 'swap',
  variable: '--font-sans',
});

const jetbrains = JetBrains_Mono({
  subsets: ['latin'],
  display: 'swap',
  variable: '--font-mono',
});

export const metadata: Metadata = {
  title: 'LumAgg — Stellar DEX Aggregator',
  description: 'Best swap rates across Stellar DEXes. Split orders for optimal execution.',
  icons: {
    icon: '/icon.svg',
    shortcut: '/icon.svg',
  },
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en" className="dark" suppressHydrationWarning>
      <body
        className={`${dmSans.variable} ${jetbrains.variable} min-h-screen antialiased text-[var(--text-primary)] font-[family-name:var(--font-sans)]`}
      >
        <Providers>
          <div className="min-h-screen flex flex-col">
            <header className="sticky top-0 z-40 shrink-0 w-full bg-[var(--bg-0)]">
              <div className="w-full px-6 sm:px-8 lg:px-14 h-[5rem] flex items-center justify-between gap-6">
                <div className="flex items-center gap-6 md:gap-9 min-w-0">
                  <Link href="/" className="flex items-center gap-2.5 group shrink-0">
                    <Image
                      src="/lumagg-mark.svg"
                      alt="LumAgg"
                      width={36}
                      height={36}
                      priority
                      className="h-9 w-9 transition-transform duration-200 group-hover:scale-[1.04]"
                    />
                    <span className="text-[18px] sm:text-[19px] font-semibold tracking-tight text-[var(--text-primary)]">
                      LumAgg
                    </span>
                  </Link>
                  <HeaderNav />
                </div>
                <div className="flex items-center gap-3 shrink-0">
                  <a
                    href={GITHUB_REPO_URL}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="hidden sm:inline-flex items-center gap-1.5 text-[15px] font-medium text-[var(--text-secondary)] hover:text-[var(--text-primary)] transition-colors"
                  >
                    <GitHubIcon className="w-4 h-4" />
                    GitHub
                  </a>
                  <a
                    href={GITHUB_REPO_URL}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="sm:hidden inline-flex items-center justify-center text-[var(--text-secondary)] hover:text-[var(--text-primary)] transition-colors"
                    aria-label="GitHub repository"
                  >
                    <GitHubIcon className="w-5 h-5" />
                  </a>
                  <HeaderWallet />
                </div>
              </div>
            </header>

            <main className="relative w-full px-6 sm:px-8 lg:px-14 pt-5 md:pt-8 pb-6 min-w-0 flex-1">
              {children}
            </main>

            <footer className="relative mt-auto w-full">
              <div className="w-full px-6 sm:px-8 lg:px-14 py-5 flex flex-col sm:flex-row items-start sm:items-center justify-between gap-3 text-[14px] sm:text-[15px] text-[var(--text-secondary)]">
                <span className="max-w-xl leading-relaxed">
                  Aggregated routing across Stellar DEXs · Best-effort quotes
                </span>
                <div className="flex items-center gap-4 shrink-0">
                  <a
                    href={DOCUMENTATION_URL}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="text-[14px] sm:text-[15px] hover:text-[var(--text-primary)] transition-colors"
                  >
                    Documentation
                  </a>
                  <a
                    href={GITHUB_REPO_URL}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="inline-flex items-center gap-1.5 text-[14px] sm:text-[15px] hover:text-[var(--text-primary)] transition-colors"
                  >
                    <GitHubIcon className="w-4 h-4" />
                    Open source
                  </a>
                </div>
              </div>
            </footer>
          </div>
        </Providers>
      </body>
    </html>
  );
}

function GitHubIcon({ className }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 24 24" fill="currentColor" aria-hidden>
      <path d="M12 0C5.37 0 0 5.37 0 12c0 5.31 3.435 9.795 8.205 11.385.6.105.825-.255.825-.57 0-.285-.015-1.04-.015-2.04-3.338.724-4.042-1.61-4.042-1.61-.546-1.385-1.335-1.755-1.335-1.755-1.087-.744.084-.729.084-.729 1.205.084 1.838 1.236 1.838 1.236 1.07 1.835 2.809 1.305 3.495.998.108-.776.417-1.305.76-1.605-2.665-.3-5.466-1.332-5.466-5.93 0-1.31.465-2.38 1.235-3.22-.135-.3-.54-1.52.105-3.17 0 0 1.005-.322 3.3 1.23.96-.27 1.98-.405 3-.405 1.02 0 2.04.135 3 .405 2.28-1.552 3.285-1.23 3.285-1.23.645 1.65.24 2.87.12 3.17.765.84 1.23 1.91 1.23 3.22 0 4.61-2.805 5.625-5.475 5.92.42.36.81 1.096.81 2.22 0 1.605-.015 2.896-.015 3.286 0 .315.21.69.825.57A12.02 12.02 0 0 0 24 12c0-6.63-5.37-12-12-12z" />
    </svg>
  );
}
