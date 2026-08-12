'use client';

import { useEffect, useState } from 'react';
import {
  getSubmitViaPreference,
  setSubmitViaPreference,
  type SubmitNetwork,
  type SubmitVia,
} from '@/lib/rpc';

export function SubmitViaToggle({ network: _network = 'public' }: { network?: SubmitNetwork }) {
  const [submitVia, setSubmitVia] = useState<SubmitVia>('official');

  useEffect(() => {
    setSubmitVia(getSubmitViaPreference());
  }, []);

  const apiHost = 'api.lumagg.xyz';

  return (
    <label className="mt-4 flex items-start gap-2 cursor-pointer select-none text-[11px] leading-snug text-[var(--text-muted)]/70 hover:text-[var(--text-muted)]">
      <input
        type="checkbox"
        className="mt-0.5 accent-[var(--accent)]"
        checked={submitVia === 'official'}
        onChange={(e) => {
          const next: SubmitVia = e.target.checked ? 'official' : 'lumagg';
          setSubmitVia(next);
          setSubmitViaPreference(next);
        }}
      />
      <span>
        Submit via LumAgg API instead of direct RPC
        {submitVia === 'lumagg' ? ` (${apiHost})` : ''}
      </span>
    </label>
  );
}
