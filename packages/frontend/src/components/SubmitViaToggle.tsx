'use client';

import { useEffect, useState } from 'react';
import {
  getSubmitViaPreference,
  setSubmitViaPreference,
  type SubmitNetwork,
  type SubmitVia,
} from '@/lib/submit-preference';

export function SubmitViaToggle({ network = 'public' }: { network?: SubmitNetwork }) {
  const [submitVia, setSubmitVia] = useState<SubmitVia>('official');

  useEffect(() => {
    setSubmitVia(getSubmitViaPreference());
  }, []);

  const setPreference = (next: SubmitVia) => {
    setSubmitVia(next);
    setSubmitViaPreference(next);
  };

  return (
    <div className="mt-4 flex flex-wrap items-center justify-between gap-2 text-[11px] text-[var(--text-muted)]">
      <span>Submit through</span>
      <div
        className="inline-flex rounded-lg border border-[var(--border)] bg-[var(--bg-0)]/40 p-0.5"
        role="group"
        aria-label="Transaction submission service"
      >
        <button
          type="button"
          onClick={() => setPreference('official')}
          aria-pressed={submitVia === 'official'}
          className={`rounded-md px-2.5 py-1 transition-colors ${
            submitVia === 'official'
              ? 'bg-[var(--surface-raised)] text-[var(--text-primary)]'
              : 'text-[var(--text-muted)] hover:text-[var(--text-secondary)]'
          }`}
        >
          {network === 'testnet' ? 'Testnet RPC' : 'Stellar RPC'}
        </button>
        <button
          type="button"
          onClick={() => setPreference('lumagg')}
          aria-pressed={submitVia === 'lumagg'}
          className={`rounded-md px-2.5 py-1 transition-colors ${
            submitVia === 'lumagg'
              ? 'bg-[var(--surface-raised)] text-[var(--text-primary)]'
              : 'text-[var(--text-muted)] hover:text-[var(--text-secondary)]'
          }`}
        >
          LumAgg API
        </button>
      </div>
    </div>
  );
}
