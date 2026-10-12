/** The logo's five level bars, for small places like the header. */
export function Bars({ className }: { className?: string }) {
  const heights = [0.45, 0.85, 0.55, 0.85, 0.45];
  return (
    <svg viewBox="0 0 16 16" className={className} aria-hidden="true">
      {heights.map((h, i) => (
        <rect key={i} x={0.5 + i * 3.25} y={8 - h * 6.4} width="2" height={h * 12.8} rx="1" className="fill-accent" />
      ))}
    </svg>
  );
}
