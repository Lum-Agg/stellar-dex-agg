export function DisclaimerBanner({ className = '' }: { className?: string }) {
  return (
    <p
      className={`text-[13px] sm:text-[14px] leading-relaxed text-[var(--text-muted)] ${className}`}
      role="status"
    >
      Early mainnet · Verify amounts before signing · Not financial advice
    </p>
  );
}
