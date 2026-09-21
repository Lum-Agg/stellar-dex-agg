import Link from 'next/link';

export default function NotFound() {
  return (
    <section className="mx-auto flex min-h-[55vh] w-full max-w-2xl items-center justify-center py-12">
      <div className="surface-panel w-full px-6 py-10 text-center sm:px-10">
        <span className="eyebrow">404 · Route unavailable</span>
        <h1 className="mt-4 text-3xl font-semibold tracking-[-0.04em] sm:text-4xl">
          This path does not exist
        </h1>
        <p className="mx-auto mt-4 max-w-lg leading-7 text-[var(--text-secondary)]">
          Check the address, or return to the swap interface to find a route across Stellar DEXes.
        </p>
        <div className="mt-8 flex flex-col justify-center gap-3 sm:flex-row">
          <Link href="/" className="btn-primary min-h-11 px-5">
            Open Swap
          </Link>
          <Link
            href="/docs"
            className="inline-flex min-h-11 items-center justify-center rounded-[0.65rem] border border-[var(--border-strong)] px-5 font-semibold text-[var(--text-primary)] transition-colors hover:bg-[var(--surface-raised)]"
          >
            Read the docs
          </Link>
        </div>
      </div>
    </section>
  );
}
