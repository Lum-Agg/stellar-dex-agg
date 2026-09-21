import type { Metadata } from 'next';

export const metadata: Metadata = {
  title: 'Public Analytics',
  description:
    'Explore LumAgg-routed volume, DEX participation, execution outcomes and network activity.',
  alternates: {
    canonical: '/stats',
  },
  openGraph: {
    title: 'Public Analytics · LumAgg',
    description:
      'Explore LumAgg-routed volume, DEX participation, execution outcomes and network activity.',
    url: '/stats',
  },
};

export default function StatsLayout({ children }: { children: React.ReactNode }) {
  return children;
}
