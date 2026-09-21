import type { Metadata } from 'next';

export const metadata: Metadata = {
  title: 'Portfolio',
  description: 'View wallet balances, estimated values and confirmed LumAgg swap history.',
  alternates: {
    canonical: '/portfolio',
  },
  openGraph: {
    title: 'Portfolio · LumAgg',
    description: 'View wallet balances, estimated values and confirmed LumAgg swap history.',
    url: '/portfolio',
  },
};

export default function PortfolioLayout({ children }: { children: React.ReactNode }) {
  return children;
}
