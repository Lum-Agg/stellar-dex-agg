'use client';

import Link from 'next/link';
import { usePathname } from 'next/navigation';
import { GITHUB_REPO_URL } from '@/lib/site';

const NAV = [
  {
    section: 'Documentation',
    items: [
      { href: '/docs', label: 'Overview', match: (p: string) => p === '/docs' },
      {
        href: '/docs/api',
        label: 'API reference',
        match: (p: string) => p.startsWith('/docs/api'),
      },
    ],
  },
  {
    section: 'Resources',
    items: [
      {
        href: `${GITHUB_REPO_URL}/blob/main/docs/openapi.yaml`,
        label: 'OpenAPI',
        external: true,
      },
      {
        href: `${GITHUB_REPO_URL}/blob/main/docs/integrator-guide.md`,
        label: 'Integrator guide',
        external: true,
      },
      {
        href: `${GITHUB_REPO_URL}/blob/main/docs/limit-orders-testnet.md`,
        label: 'Limit orders (testnet)',
        external: true,
      },
      { href: GITHUB_REPO_URL, label: 'GitHub', external: true },
    ],
  },
] as const;

export function DocsSidebar() {
  const pathname = usePathname() || '/docs';

  return (
    <aside className="docs-sidebar">
      <p className="docs-sidebar-label">Developer docs</p>
      {NAV.map((group) => (
        <div key={group.section} className="docs-sidebar-group">
          <p className="docs-sidebar-section">{group.section}</p>
          <ul className="docs-sidebar-list">
            {group.items.map((item) => {
              const active = 'match' in item && item.match(pathname);
              if ('external' in item && item.external) {
                return (
                  <li key={item.href}>
                    <a
                      href={item.href}
                      target="_blank"
                      rel="noopener noreferrer"
                      className="docs-sidebar-link docs-sidebar-link--external"
                    >
                      {item.label}
                    </a>
                  </li>
                );
              }
              return (
                <li key={item.href}>
                  <Link
                    href={item.href}
                    className={`docs-sidebar-link${active ? ' docs-sidebar-link--active' : ''}`}
                    aria-current={active ? 'page' : undefined}
                  >
                    {item.label}
                  </Link>
                </li>
              );
            })}
          </ul>
        </div>
      ))}
    </aside>
  );
}
