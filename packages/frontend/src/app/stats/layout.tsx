import type { Metadata } from 'next';

export const metadata: Metadata = {
  title: 'Public Analytics',
  description:
    'See user swaps and arbitrage separately, alongside combined routed volume and execution outcomes.',
  alternates: {
    canonical: '/stats',
  },
  openGraph: {
    title: 'Public Analytics · LumAgg',
    description:
      'See user swaps and arbitrage separately, alongside combined routed volume and execution outcomes.',
    url: '/stats',
  },
};

export default function StatsLayout({ children }: { children: React.ReactNode }) {
  return children;
}
