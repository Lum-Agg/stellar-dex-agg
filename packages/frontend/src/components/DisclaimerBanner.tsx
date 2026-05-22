export function DisclaimerBanner({ className = '' }: { className?: string }) {
  return (
    <div
      className={`rounded-xl border border-amber-500/30 bg-amber-500/10 px-4 py-3 text-sm text-amber-100/90 ${className}`}
      role="status"
    >
      <p className="font-medium text-amber-200">Use with caution</p>
      <p className="text-xs text-amber-200/80 mt-1 leading-relaxed">
        LumAgg is under active development. Quotes and routes may change; always verify amounts
        and contract addresses before signing. Not financial advice.
      </p>
    </div>
  );
}
