import type { Metadata } from 'next';
import { DocsSidebar } from '@/components/docs/DocsSidebar';

export const metadata: Metadata = {
  title: 'Developer Documentation',
  description: 'Integrate the LumAgg API and SDK, or self-host the Stellar routing stack.',
  alternates: {
    canonical: '/docs',
  },
  openGraph: {
    title: 'Developer Documentation · LumAgg',
    description: 'Integrate the LumAgg API and SDK, or self-host the Stellar routing stack.',
    url: '/docs',
  },
};

export default function DocsLayout({ children }: { children: React.ReactNode }) {
  return (
    <div className="docs-shell">
      <DocsSidebar />
      <div className="docs-main">{children}</div>
    </div>
  );
}
