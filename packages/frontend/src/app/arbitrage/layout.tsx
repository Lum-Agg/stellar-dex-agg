import type { Metadata } from 'next';

export const metadata: Metadata = {
  title: 'Arbitrage Analytics',
  description:
    'View confirmed LumAgg arbitrage round trips, gross surplus, execution status and failure diagnostics.',
  alternates: {
    canonical: '/arbitrage',
  },
  openGraph: {
    title: 'Arbitrage Analytics · LumAgg',
    description:
      'View confirmed LumAgg arbitrage round trips, gross surplus, execution status and failure diagnostics.',
    url: '/arbitrage',
  },
};

export default function ArbitrageLayout({ children }: { children: React.ReactNode }) {
  return children;
}
