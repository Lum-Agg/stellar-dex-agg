export function DisclaimerBanner({ className = '' }: { className?: string }) {
  return (
    <div
      className={`rounded-lg border border-white/[0.08] bg-zinc-900/60 px-4 py-3 text-sm text-zinc-400 ${className}`}
      role="status"
    >
      <p className="font-medium text-zinc-300">Use with caution</p>
      <p className="text-[12px] text-zinc-500 mt-1 leading-relaxed">
        LumAgg is under active development. Quotes and routes may change; always verify amounts
        and contract addresses before signing. Not financial advice.
      </p>
    </div>
  );
}
