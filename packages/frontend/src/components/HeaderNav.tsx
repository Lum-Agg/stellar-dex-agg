'use client';

import Link from 'next/link';
import { usePathname } from 'next/navigation';

const LINKS = [
  { href: '/', label: 'Swap', match: (path: string) => path === '/' },
  {
    href: '/portfolio',
    label: 'Portfolio',
    match: (path: string) => path.startsWith('/portfolio'),
  },
  { href: '/docs', label: 'Docs', match: (path: string) => path.startsWith('/docs') },
  { href: '/stats', label: 'Stats', match: (path: string) => path.startsWith('/stats') },
] as const;

export function HeaderNav() {
  const pathname = usePathname() || '/';

  return (
    <nav className="hidden sm:flex items-center gap-5 md:gap-7 text-[16px] sm:text-[17px] font-medium text-[var(--text-secondary)]">
      {LINKS.map((link) => {
        const active = link.match(pathname);
        return (
          <Link
            key={link.href}
            href={link.href}
            className={`transition-colors ${
              active ? 'text-[var(--text-primary)]' : 'hover:text-[var(--text-primary)]'
            }`}
            aria-current={active ? 'page' : undefined}
          >
            {link.label}
          </Link>
        );
      })}
    </nav>
  );
}
