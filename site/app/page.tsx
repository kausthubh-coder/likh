// Placeholder until the first release: the real landing page (download
// buttons from GitHub Releases, the live pill demo) comes with it.

const REPO = "https://github.com/kausthubh-coder/murmur";

function Mark() {
  const bars = [0.45, 0.85, 0.55, 0.85, 0.45];
  return (
    <svg width="64" height="40" viewBox="0 0 64 40" aria-hidden="true">
      <rect x="0.5" y="0.5" width="63" height="39" rx="19.5" className="fill-surface" stroke="oklch(1 0 0 / 0.16)" />
      {bars.map((h, i) => (
        <rect key={i} x={15 + i * 8} y={20 - h * 11} width="4" height={h * 22} rx="2" className="fill-accent" />
      ))}
    </svg>
  );
}

export default function Home() {
  return (
    <main className="mx-auto flex w-full max-w-2xl flex-1 flex-col justify-center gap-6 px-6 py-24">
      <Mark />
      <div>
        <h1 className="text-[56px] leading-[60px] font-semibold tracking-[-0.03em]">murmur</h1>
        <p className="text-[34px] leading-[38px] font-semibold tracking-[-0.02em] text-text-muted">Talk. It types.</p>
      </div>
      <p className="max-w-[52ch] text-[15px] leading-[22px] text-text-muted text-pretty">
        Native dictation for Windows (and macOS, in alpha). Hold a key, say it, let go: your words land wherever
        you&apos;re typing, transcribed with the ChatGPT plan you already have through Codex. No web view, idles at
        near zero.
      </p>
      <div className="flex gap-2">
        <a
          href={REPO}
          className="rounded-[10px] bg-accent px-4 py-2 text-[13px] font-medium text-on-accent transition-transform duration-100 ease-(--ease-snappy) active:scale-[0.97]"
        >
          Follow on GitHub
        </a>
        <span className="rounded-[10px] bg-elevated px-4 py-2 font-mono text-[12px] leading-[22px] text-text-subtle">
          First release soon
        </span>
      </div>
    </main>
  );
}
