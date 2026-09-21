'use client';

import { useEffect, useId, useRef, useState } from 'react';
import Link from 'next/link';
import { usePathname } from 'next/navigation';
import { DOCUMENTATION_URL } from '@/lib/site';
import { GITHUB_REPO_URL } from '@/lib/site';

const DOCS_LINKS = [
  {
    label: 'Quickstart',
    detail: 'Quote, build and sign your first swap',
    href: '/docs',
  },
  {
    label: 'Interactive API reference',
    detail: 'Try quote and build_tx endpoints',
    href: '/docs/api',
  },
  {
    label: 'Self-host the Aggregator',
    detail: 'API, worker, Redis and routing stack',
    href: `${GITHUB_REPO_URL}/blob/main/docs/self-hosted-aggregator-quickstart.md`,
  },
  {
    label: 'Self-host Arbitrage Bot',
    detail: 'Callers, Vault and atomic round trips',
    href: `${GITHUB_REPO_URL}/blob/main/docs/arbitrage-deployment.md`,
  },
  {
    label: 'TypeScript SDK & examples',
    detail: 'Install the SDK and build an integration',
    href: 'https://www.npmjs.com/package/@lumagg/sdk',
  },
  {
    label: 'Contract addresses',
    detail: 'Mainnet and testnet deployment records',
    href: `${GITHUB_REPO_URL}/blob/main/docs/contracts-deployment.md`,
  },
  {
    label: 'Public analytics',
    detail: 'Stats and confirmed arbitrage activity',
    href: '/stats',
  },
] as const;

const PRIMARY_LINKS = [
  { href: '/', label: 'Swap', match: (path: string) => path === '/' },
  {
    href: '/portfolio',
    label: 'Portfolio',
    match: (path: string) => path.startsWith('/portfolio'),
  },
] as const;

const SECONDARY_LINKS = [
  { href: '/stats', label: 'Stats', match: (path: string) => path.startsWith('/stats') },
  {
    href: '/arbitrage',
    label: 'Arbitrage',
    match: (path: string) => path.startsWith('/arbitrage'),
  },
] as const;

type DocsLinkItem = (typeof DOCS_LINKS)[number];

function DocsLink({
  link,
  mobile = false,
  onSelect,
}: {
  link: DocsLinkItem;
  mobile?: boolean;
  onSelect: () => void;
}) {
  const className = mobile
    ? 'block px-4 py-2.5 text-[15px] font-medium text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:bg-white/[0.03] transition-colors'
    : 'docs-dropdown-link';
  const content = mobile ? (
    link.label
  ) : (
    <>
      <strong>{link.label}</strong>
      <small>{link.detail}</small>
    </>
  );

  if (link.href.startsWith('/')) {
    return (
      <Link href={link.href} className={className} role="menuitem" onClick={onSelect}>
        {content}
      </Link>
    );
  }

  return (
    <a
      href={link.href}
      target="_blank"
      rel="noopener noreferrer"
      className={className}
      role="menuitem"
      onClick={onSelect}
    >
      {content}
    </a>
  );
}

export function HeaderNav() {
  const pathname = usePathname() || '/';
  const [open, setOpen] = useState(false);
  const menuId = useId();
  const rootRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    setOpen(false);
  }, [pathname]);

  useEffect(() => {
    if (!open) return;
    const previousOverflow = document.body.style.overflow;
    const onPointerDown = (event: MouseEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) {
        setOpen(false);
      }
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        setOpen(false);
      }
    };
    document.body.style.overflow = 'hidden';
    document.addEventListener('mousedown', onPointerDown);
    document.addEventListener('keydown', onKeyDown);
    return () => {
      document.body.style.overflow = previousOverflow;
      document.removeEventListener('mousedown', onPointerDown);
      document.removeEventListener('keydown', onKeyDown);
    };
  }, [open]);

  return (
    <div ref={rootRef} className="relative">
      {/* Desktop */}
      <nav className="hidden md:flex items-center gap-5 md:gap-7 text-[16px] sm:text-[17px] font-medium text-[var(--text-secondary)]">
        {PRIMARY_LINKS.map((link) => {
          const active = link.match(pathname);
          return (
            <Link
              key={link.href}
              href={link.href}
              className={`nav-link transition-colors ${
                active ? 'text-[var(--text-primary)]' : 'hover:text-[var(--text-primary)]'
              }`}
              aria-current={active ? 'page' : undefined}
            >
              {link.label}
            </Link>
          );
        })}

        {SECONDARY_LINKS.map((link) => {
          const active = link.match(pathname);
          return (
            <Link
              key={link.href}
              href={link.href}
              className={`nav-link transition-colors ${
                active ? 'text-[var(--text-primary)]' : 'hover:text-[var(--text-primary)]'
              }`}
              aria-current={active ? 'page' : undefined}
            >
              {link.label}
            </Link>
          );
        })}

        <DocsMenu active={pathname.startsWith('/docs')} />
      </nav>

      {/* Mobile */}
      <button
        type="button"
        className="md:hidden inline-flex items-center justify-center w-10 h-10 -ml-1 rounded-lg text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:bg-white/[0.04] transition-colors"
        aria-label={open ? 'Close menu' : 'Open menu'}
        aria-expanded={open}
        aria-controls={menuId}
        onClick={() => setOpen((v) => !v)}
      >
        <MenuIcon open={open} />
      </button>

      {open && (
        <nav
          id={menuId}
          className="md:hidden absolute left-0 top-[calc(100%+0.5rem)] z-50 min-w-[13rem] max-h-[calc(100vh-6rem)] overflow-y-auto rounded-xl border border-white/10 bg-[var(--bg-0)] py-1.5 shadow-xl shadow-black/40"
        >
          {PRIMARY_LINKS.map((link) => {
            const active = link.match(pathname);
            return (
              <Link
                key={link.href}
                href={link.href}
                className={`block px-4 py-2.5 text-[15px] font-medium transition-colors ${
                  active
                    ? 'text-[var(--text-primary)] bg-white/[0.04]'
                    : 'text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:bg-white/[0.03]'
                }`}
                aria-current={active ? 'page' : undefined}
                onClick={() => setOpen(false)}
              >
                {link.label}
              </Link>
            );
          })}

          {SECONDARY_LINKS.map((link) => {
            const active = link.match(pathname);
            return (
              <Link
                key={link.href}
                href={link.href}
                className={`block px-4 py-2.5 text-[15px] font-medium transition-colors ${
                  active
                    ? 'text-[var(--text-primary)] bg-white/[0.04]'
                    : 'text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:bg-white/[0.03]'
                }`}
                aria-current={active ? 'page' : undefined}
                onClick={() => setOpen(false)}
              >
                {link.label}
              </Link>
            );
          })}

          <div className="border-t border-white/[0.06] mt-1 pt-1">
            <div
              className={`px-4 py-2 text-[11px] font-semibold uppercase tracking-[0.14em] ${
                pathname.startsWith('/docs') ? 'text-[var(--accent)]' : 'text-[var(--text-muted)]'
              }`}
            >
              Docs
            </div>
            {DOCS_LINKS.map((link) => (
              <DocsLink key={link.label} link={link} mobile onSelect={() => setOpen(false)} />
            ))}
            <a
              href={DOCUMENTATION_URL}
              target="_blank"
              rel="noopener noreferrer"
              className="block px-4 py-2.5 text-[15px] font-medium text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:bg-white/[0.03] transition-colors"
              onClick={() => setOpen(false)}
            >
              Full documentation
            </a>
          </div>
        </nav>
      )}
    </div>
  );
}

function DocsMenu({ active }: { active: boolean }) {
  const [docsOpen, setDocsOpen] = useState(false);

  return (
    <div
      className="relative"
      onBlur={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget as Node | null)) {
          setDocsOpen(false);
        }
      }}
    >
      <button
        type="button"
        className={`nav-link inline-flex items-center gap-1 transition-colors ${
          docsOpen || active ? 'text-[var(--text-primary)]' : 'hover:text-[var(--text-primary)]'
        }`}
        aria-current={active ? 'page' : undefined}
        aria-expanded={docsOpen}
        aria-haspopup="menu"
        onKeyDown={(event) => {
          if (event.key === 'Escape') setDocsOpen(false);
        }}
        onClick={() => setDocsOpen((value) => !value)}
      >
        Docs
        <ChevronIcon open={docsOpen} />
      </button>
      {docsOpen && (
        <div className="docs-dropdown" role="menu">
          {DOCS_LINKS.map((link) => (
            <DocsLink key={link.label} link={link} onSelect={() => setDocsOpen(false)} />
          ))}
          <a
            href={DOCUMENTATION_URL}
            target="_blank"
            rel="noopener noreferrer"
            className="docs-dropdown-link docs-dropdown-link--last"
            role="menuitem"
            onClick={() => setDocsOpen(false)}
          >
            <strong>Full documentation</strong>
            <small>All product and contract guides</small>
          </a>
        </div>
      )}
    </div>
  );
}

function ChevronIcon({ open }: { open: boolean }) {
  return (
    <svg
      className={`h-3 w-3 transition-transform ${open ? 'rotate-180' : ''}`}
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      aria-hidden
    >
      <path d="m3.5 6 4.5 4 4.5-4" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

function MenuIcon({ open }: { open: boolean }) {
  return (
    <svg className="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" aria-hidden>
      {open ? (
        <path strokeWidth="2" strokeLinecap="round" d="M6 6l12 12M18 6L6 18" />
      ) : (
        <path strokeWidth="2" strokeLinecap="round" d="M4 7h16M4 12h16M4 17h16" />
      )}
    </svg>
  );
}
