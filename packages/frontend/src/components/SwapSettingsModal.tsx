'use client';

import { useEffect, useId, useRef, useState, type ReactNode } from 'react';
import { createPortal } from 'react-dom';
import {
  formatSlippageLabel,
  MAX_HOPS_MAX,
  MAX_HOPS_MIN,
  MAX_SPLITS_MAX,
  MAX_SPLITS_MIN,
  parseSlippageInput,
  SLIPPAGE_PRESETS,
  type SwapSettings,
} from '@/lib/swap-settings';

type Props = {
  open: boolean;
  settings: SwapSettings;
  onClose: () => void;
  onChange: (next: SwapSettings) => void;
};

function InfoTip({ label, children }: { label: string; children: ReactNode }) {
  const [open, setOpen] = useState(false);
  const tipId = useId();
  const rootRef = useRef<HTMLSpanElement>(null);

  useEffect(() => {
    if (!open) return;
    const onDoc = (e: MouseEvent) => {
      if (!rootRef.current?.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.stopPropagation();
        setOpen(false);
      }
    };
    document.addEventListener('mousedown', onDoc);
    document.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('mousedown', onDoc);
      document.removeEventListener('keydown', onKey);
    };
  }, [open]);

  return (
    <span
      ref={rootRef}
      className="relative inline-flex"
      onMouseEnter={() => setOpen(true)}
      onMouseLeave={() => setOpen(false)}
    >
      <button
        type="button"
        aria-label={label}
        aria-expanded={open}
        aria-controls={tipId}
        onClick={(e) => {
          e.stopPropagation();
          setOpen((v) => !v);
        }}
        className="inline-flex h-4 w-4 items-center justify-center rounded-full border border-[var(--border-strong)] text-[10px] leading-none text-[var(--text-muted)] transition-colors hover:border-[var(--accent)]/50 hover:text-[var(--text-primary)]"
      >
        i
      </button>
      {open && (
        <span
          id={tipId}
          role="tooltip"
          className="absolute left-0 top-[calc(100%+6px)] z-10 w-56 rounded-lg border border-[var(--border)] bg-[var(--surface-raised)] px-2.5 py-2 text-[11px] leading-snug text-[var(--text-secondary)] shadow-lg"
        >
          {children}
        </span>
      )}
    </span>
  );
}

export function SwapSettingsModal({ open, settings, onClose, onChange }: Props) {
  const [customSlippage, setCustomSlippage] = useState('');
  const [customMode, setCustomMode] = useState(
    () => !SLIPPAGE_PRESETS.includes(settings.slippage as (typeof SLIPPAGE_PRESETS)[number]),
  );

  useEffect(() => {
    if (!open) return;
    const isPreset = SLIPPAGE_PRESETS.includes(
      settings.slippage as (typeof SLIPPAGE_PRESETS)[number],
    );
    setCustomMode(!isPreset);
    setCustomSlippage(isPreset ? '' : String(settings.slippage));
  }, [open, settings.slippage]);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [open, onClose]);

  if (!open || typeof window === 'undefined') return null;

  const setSlippage = (n: number) => {
    onChange({ ...settings, slippage: n });
  };

  return createPortal(
    <div
      className="fixed inset-0 z-[200] flex items-center justify-center bg-black/75 backdrop-blur-[2px] p-4"
      onClick={onClose}
      role="presentation"
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="swap-settings-title"
        className="w-full max-w-md rounded-2xl border border-[var(--border)] bg-[var(--surface)] shadow-xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between border-b border-[var(--border)] px-5 py-4">
          <h3
            id="swap-settings-title"
            className="text-[15px] font-semibold text-[var(--text-primary)]"
          >
            Swap Settings
          </h3>
          <button
            type="button"
            onClick={onClose}
            className="text-[var(--text-muted)] hover:text-[var(--text-primary)]"
            aria-label="Close settings"
          >
            <svg className="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d="M6 18L18 6M6 6l12 12"
              />
            </svg>
          </button>
        </div>

        <div className="space-y-6 overflow-visible px-5 py-5">
          <section>
            <div className="mb-2.5 flex items-center gap-1.5">
              <span className="text-[13px] font-medium text-[var(--text-secondary)]">
                Max slippage
              </span>
              <InfoTip label="About max slippage">
                Maximum price movement you accept versus the quoted output. If execution would
                deliver less than the minimum, the swap fails.
              </InfoTip>
            </div>
            <div className="flex items-center gap-0.5 rounded-full border border-[var(--border)] bg-[var(--bg-0)]/50 p-0.5">
              {SLIPPAGE_PRESETS.map((s) => {
                const active = !customMode && settings.slippage === s;
                return (
                  <button
                    key={s}
                    type="button"
                    onClick={() => {
                      setCustomMode(false);
                      setCustomSlippage('');
                      setSlippage(s);
                    }}
                    className={`flex-1 rounded-full px-3 py-1.5 text-[13px] transition-colors ${
                      active
                        ? 'bg-[var(--surface-raised)] text-[var(--text-primary)]'
                        : 'text-[var(--text-muted)] hover:text-[var(--text-secondary)]'
                    }`}
                  >
                    {formatSlippageLabel(s)}
                  </button>
                );
              })}
              <label
                className={`flex min-w-[5.5rem] items-center gap-0.5 rounded-full px-2 py-1 transition-colors ${
                  customMode
                    ? 'bg-[var(--surface-raised)] text-[var(--text-primary)]'
                    : 'text-[var(--text-muted)]'
                }`}
              >
                <input
                  type="text"
                  inputMode="decimal"
                  aria-label="Custom slippage percent"
                  placeholder="Custom"
                  value={customMode ? customSlippage : ''}
                  onFocus={() => {
                    setCustomMode(true);
                    setCustomSlippage(customSlippage || String(settings.slippage));
                  }}
                  onChange={(e) => {
                    const val = e.target.value;
                    if (!/^\d*\.?\d*$/.test(val)) return;
                    setCustomMode(true);
                    setCustomSlippage(val);
                    const parsed = parseSlippageInput(val);
                    if (parsed !== null) setSlippage(parsed);
                  }}
                  onBlur={() => {
                    const parsed = parseSlippageInput(customSlippage);
                    if (parsed === null) {
                      const preset = SLIPPAGE_PRESETS.find((s) => s === settings.slippage);
                      if (preset !== undefined) {
                        setCustomMode(false);
                        setCustomSlippage('');
                      } else {
                        setCustomSlippage(String(settings.slippage));
                      }
                      return;
                    }
                    setSlippage(parsed);
                    setCustomSlippage(String(parsed));
                  }}
                  className="w-12 bg-transparent text-right text-[13px] outline-none placeholder-[var(--text-muted)]/70 tabular-nums"
                />
                <span className="shrink-0 text-[13px]">%</span>
              </label>
            </div>
          </section>

          <section className="flex items-center justify-between gap-4">
            <div>
              <div className="flex items-center gap-1.5">
                <span className="text-[13px] font-medium text-[var(--text-secondary)]">
                  Max hops
                </span>
                <InfoTip label="About max hops">
                  Maximum pools in a single path. Higher finds more routes but uses more compute
                  and can raise fees.
                </InfoTip>
              </div>
              <p className="mt-0.5 text-[11px] text-[var(--text-muted)]">
                {MAX_HOPS_MIN}–{MAX_HOPS_MAX}
              </p>
            </div>
            <input
              type="number"
              min={MAX_HOPS_MIN}
              max={MAX_HOPS_MAX}
              step={1}
              value={settings.maxHops}
              onChange={(e) => {
                const n = Number(e.target.value);
                if (!Number.isFinite(n)) return;
                onChange({
                  ...settings,
                  maxHops: Math.min(MAX_HOPS_MAX, Math.max(MAX_HOPS_MIN, Math.round(n))),
                });
              }}
              className="w-16 rounded-lg border border-[var(--border)] bg-[var(--bg-0)]/50 px-2 py-1.5 text-center text-[13px] tabular-nums text-[var(--text-primary)] outline-none focus:border-[var(--accent)]"
            />
          </section>

          <section className="flex items-center justify-between gap-4">
            <div>
              <div className="flex items-center gap-1.5">
                <span className="text-[13px] font-medium text-[var(--text-secondary)]">
                  Max splits
                </span>
                <InfoTip label="About max splits">
                  Maximum parallel paths in one swap. Higher may improve price but raises Soroban
                  resource fees and Budget risk.
                </InfoTip>
              </div>
              <p className="mt-0.5 text-[11px] text-[var(--text-muted)]">
                {MAX_SPLITS_MIN}–{MAX_SPLITS_MAX}
              </p>
            </div>
            <input
              type="number"
              min={MAX_SPLITS_MIN}
              max={MAX_SPLITS_MAX}
              step={1}
              value={settings.maxSplits}
              onChange={(e) => {
                const n = Number(e.target.value);
                if (!Number.isFinite(n)) return;
                onChange({
                  ...settings,
                  maxSplits: Math.min(MAX_SPLITS_MAX, Math.max(MAX_SPLITS_MIN, Math.round(n))),
                });
              }}
              className="w-16 rounded-lg border border-[var(--border)] bg-[var(--bg-0)]/50 px-2 py-1.5 text-center text-[13px] tabular-nums text-[var(--text-primary)] outline-none focus:border-[var(--accent)]"
            />
          </section>
        </div>
      </div>
    </div>,
    document.body,
  );
}
