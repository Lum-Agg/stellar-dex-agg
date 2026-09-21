import type { Metadata } from 'next';

export const metadata: Metadata = {
  title: {
    absolute: 'API Reference · LumAgg',
  },
  description: 'Explore and test the LumAgg quote, transaction building and analytics endpoints.',
  alternates: {
    canonical: '/docs/api',
  },
  openGraph: {
    title: 'API Reference · LumAgg',
    description: 'Explore and test the LumAgg quote, transaction building and analytics endpoints.',
    url: '/docs/api',
  },
};

export default function ApiReferenceLayout({ children }: { children: React.ReactNode }) {
  return children;
}
