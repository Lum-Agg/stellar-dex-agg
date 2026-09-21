import type { MetadataRoute } from 'next';

const ROUTES = ['', '/portfolio', '/stats', '/arbitrage', '/docs', '/docs/api'] as const;

export const dynamic = 'force-static';

export default function sitemap(): MetadataRoute.Sitemap {
  const lastModified = new Date();
  return ROUTES.map((route) => ({
    url: `https://lumagg.xyz${route}`,
    lastModified,
    changeFrequency:
      route === '' || route === '/stats' || route === '/arbitrage' ? 'daily' : 'weekly',
    priority: route === '' ? 1 : route === '/stats' || route === '/arbitrage' ? 0.8 : 0.6,
  }));
}
