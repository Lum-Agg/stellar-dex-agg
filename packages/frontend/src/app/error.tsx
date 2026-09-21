'use client';

import Link from 'next/link';
import { useEffect } from 'react';

export default function ErrorPage({
  error,
  reset,
}: {
  error: Error & { digest?: string };
  reset: () => void;
}) {
  useEffect(() => {
    console.error('LumAgg route error', error);
  }, [error]);

  return (
    <section className="mx-auto flex min-h-[55vh] w-full max-w-2xl items-center justify-center py-12">
      <div className="surface-panel w-full px-6 py-10 text-center sm:px-10">
        <span className="eyebrow">Temporary error</span>
        <h1 className="mt-4 text-3xl font-semibold tracking-[-0.04em] sm:text-4xl">
          This page could not load
        </h1>
        <p className="mx-auto mt-4 max-w-lg leading-7 text-[var(--text-secondary)]">
          LumAgg encountered an unexpected client error. Your wallet remains in your control and no
          transaction was submitted by this screen.
        </p>
        <div className="mt-8 flex flex-col justify-center gap-3 sm:flex-row">
          <button type="button" onClick={reset} className="btn-primary min-h-11 px-5">
            Try again
          </button>
          <Link
            href="/"
            className="inline-flex min-h-11 items-center justify-center rounded-[0.65rem] border border-[var(--border-strong)] px-5 font-semibold text-[var(--text-primary)] transition-colors hover:bg-[var(--surface-raised)]"
          >
            Return to Swap
          </Link>
        </div>
      </div>
    </section>
  );
}
