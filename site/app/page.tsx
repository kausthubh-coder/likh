import { Demo } from "./demo";
import { FlockMark } from "./flock-mark";

const REPO = "https://github.com/kausthubh-coder/murmur";
/** The published build. Bump with each release; the page reads everything else from here. */
const RELEASE = {
  version: "0.1.0-alpha.1",
  windows: `${REPO}/releases/download/v0.1.0-alpha.1/murmur-windows-x64.exe`,
  size: "8 MB",
};

/** Measured on the release build, idle, Windows 11 (docs/plan.md, Measured so far). */
const STATS = [
  { value: "14 MB", label: "of memory while it waits" },
  { value: "0%", label: "CPU while it waits" },
  { value: RELEASE.size, label: "the whole download" },
  { value: "0", label: "browser engines inside" },
];

const NEXT = [
  "Hands-free: double-tap, talk, tap to stop",
  "History, and paste the last one again",
  "Pick your microphone",
  "Start with Windows",
  "Quiet sounds for start and stop",
  "Dictations longer than ten minutes",
  "A dictionary for your own words",
  "macOS, out of alpha",
];

export default function Home() {
  return (
    <div className="mx-auto w-full max-w-[1080px] px-5 sm:px-8">
      <header className="flex h-16 items-center justify-between">
        <a href="#" className="flex items-center gap-2 text-[15px] font-semibold tracking-[-0.02em]">
          <Bars className="h-4" />
          murmur
        </a>
        <nav className="-mr-3 flex text-[13px] font-medium text-text-muted">
          <a href="#download" className="rounded-[10px] px-3 py-2 hover:text-text">
            Download
          </a>
          <a href={REPO} className="rounded-[10px] px-3 py-2 hover:text-text">
            GitHub
          </a>
        </nav>
      </header>

      <main>
        <section className="flex flex-col items-center pt-12 text-center sm:pt-20">
          <FlockMark />
          <h1 className="mt-6 text-[44px] leading-[48px] font-semibold tracking-[-0.03em] text-balance sm:text-[72px] sm:leading-[76px]">
            Talk. It types.
          </h1>
          <p className="mt-5 max-w-[30em] text-[17px] leading-[26px] text-pretty text-text-muted">
            Hold Ctrl + Space, talk, let go. Your words land wherever you&apos;re typing, on the ChatGPT plan you already have.
          </p>
          <DownloadButton className="mt-8" />
        </section>

        <section className="mt-16 sm:mt-24">
          <Demo />
        </section>

        <section className="mt-24 grid gap-8 sm:mt-32 sm:grid-cols-3">
          <Step n={1} title="Hold Ctrl + Space" line="In any app. Your cursor stays where it is." />
          <Step n={2} title="Talk" line="The pill's bars move with your voice." />
          <Step n={3} title="Let go" line="The text pastes at the cursor. Esc cancels." />
        </section>

        <section className="mt-24 sm:mt-32">
          <h2 className="text-[28px] leading-[34px] font-semibold tracking-[-0.02em] text-balance sm:text-[34px] sm:leading-[40px]">
            Light enough to forget it&apos;s running.
          </h2>
          <dl className="mt-10 grid grid-cols-2 gap-x-8 gap-y-10 sm:grid-cols-4">
            {STATS.map((stat) => (
              <div key={stat.label}>
                <dt className="sr-only">{stat.label}</dt>
                <dd className="text-[44px] leading-[48px] font-semibold tracking-[-0.03em] tabular-nums">{stat.value}</dd>
                <dd className="mt-1 text-[15px] leading-[22px] text-text-muted">{stat.label}</dd>
              </div>
            ))}
          </dl>
          <Shrink />
        </section>

        <section className="mt-24 grid gap-8 sm:mt-32 sm:grid-cols-3">
          <Fact title="Your ChatGPT plan" line="Signs in through Codex. No new account, no API key, nothing to pay." />
          <Fact title="Your voice goes one place" line="Straight to ChatGPT. No murmur account, no analytics." />
          <Fact title="Never in the way" line="Lives in the tray. The mic is open only while you hold the keys." />
        </section>

        <section className="mt-24 sm:mt-32">
          <h2 className="text-[28px] leading-[34px] font-semibold tracking-[-0.02em]">Coming next</h2>
          <ul className="mt-6 grid gap-x-8 gap-y-3 sm:grid-cols-2">
            {NEXT.map((item) => (
              <li key={item} className="flex items-center gap-3 text-[15px] leading-[22px] text-text-muted">
                <span className="size-1.5 flex-none rounded-full bg-accent" />
                {item}
              </li>
            ))}
          </ul>
        </section>

        <section id="download" className="mt-24 scroll-mt-8 sm:mt-32">
          <div className="flex flex-col items-start gap-8 sm:flex-row sm:items-end sm:justify-between">
            <div>
              <h2 className="text-[28px] leading-[34px] font-semibold tracking-[-0.02em]">Download</h2>
              <p className="mt-2 text-[15px] leading-[22px] text-text-muted">
                Windows 10 and 11 · {RELEASE.size} · v{RELEASE.version}
              </p>
            </div>
            <DownloadButton />
          </div>
          <div className="mt-8 grid gap-6 text-[15px] leading-[22px] text-text-muted sm:grid-cols-3">
            <p>
              <span className="text-text">1.</span> Install{" "}
              <a href="https://developers.openai.com/codex" className="text-text underline-offset-4 hover:underline">
                Codex
              </a>{" "}
              and sign in once.
            </p>
            <p>
              <span className="text-text">2.</span> Run murmur. It isn&apos;t signed yet, so Windows asks first: More info, then
              Run anyway.
            </p>
            <p>
              <span className="text-text">3.</span> Hold Ctrl + Space anywhere. On a Mac,{" "}
              <a href={REPO} className="text-text underline-offset-4 hover:underline">
                build from source
              </a>{" "}
              (alpha).
            </p>
          </div>
        </section>

        <section className="mt-24 max-w-[44em] sm:mt-32">
          <h2 className="text-[28px] leading-[34px] font-semibold tracking-[-0.02em]">Questions</h2>
          <div className="mt-6 divide-y divide-elevated">
            <Question q="Is it free?">
              Yes. murmur is MIT licensed, and transcription runs on the ChatGPT plan you already have through Codex.
            </Question>
            <Question q="Is this an official OpenAI app?">
              No. murmur uses the transcription endpoint the Codex desktop app uses for its own dictation. OpenAI can change it
              at any time; if that happens, murmur ships an update or stops working.
            </Question>
            <Question q="How long can I talk?">
              Up to ten minutes at a time, the same as Codex. The pill counts down your last minute.
            </Question>
          </div>
        </section>
      </main>

      <footer className="mt-24 flex flex-col gap-2 py-10 text-[13px] text-text-subtle sm:mt-32 sm:flex-row sm:justify-between">
        <span>murmur · MIT · named after starlings moving as one at dusk</span>
        <a href={REPO} className="hover:text-text">
          Source on GitHub
        </a>
      </footer>
    </div>
  );
}

function DownloadButton({ className = "" }: { className?: string }) {
  return (
    <a
      href={RELEASE.windows}
      className={`inline-flex items-center gap-2 rounded-[10px] bg-accent px-5 py-3 text-[15px] font-medium text-on-accent transition-transform duration-100 ease-(--ease-snappy) hover:bg-accent/90 active:scale-[0.97] ${className}`}
    >
      <svg
        width="16"
        height="16"
        viewBox="0 0 16 16"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.6"
        strokeLinecap="round"
        strokeLinejoin="round"
        aria-hidden="true"
      >
        <path d="M8 2.5v8M4.5 7 8 10.5 11.5 7M3 13.5h10" />
      </svg>
      Download for Windows
    </a>
  );
}

/** The one comparison we can measure honestly: murmur against its own first build. */
function Shrink() {
  const rows = [
    { label: "First build, drawn on the GPU", mb: 313, tone: "bg-text-subtle" },
    { label: "murmur today", mb: 14, tone: "bg-accent" },
  ];
  return (
    <figure className="mt-12 max-w-[640px]">
      <div className="space-y-3">
        {rows.map((row) => (
          <div key={row.label} className="grid grid-cols-[1fr_auto] items-center gap-x-4 gap-y-1.5">
            <span className="text-[13px] text-text-muted">{row.label}</span>
            <span className="text-[13px] font-medium tabular-nums">{row.mb} MB</span>
            <span className="col-span-2 h-2 rounded-full bg-surface">
              <span
                className={`block h-full rounded-full ${row.tone}`}
                style={{ width: `${Math.max(2, (row.mb / 313) * 100)}%` }}
              />
            </span>
          </div>
        ))}
      </div>
      <figcaption className="mt-3 text-[13px] text-text-subtle">
        Memory while idle. We dropped the GPU renderer and draw everything on the CPU.
      </figcaption>
    </figure>
  );
}

function Bars({ className }: { className?: string }) {
  const heights = [0.45, 0.85, 0.55, 0.85, 0.45];
  return (
    <svg viewBox="0 0 16 16" className={className} aria-hidden="true">
      {heights.map((h, i) => (
        <rect key={i} x={0.5 + i * 3.25} y={8 - h * 6.4} width="2" height={h * 12.8} rx="1" className="fill-accent" />
      ))}
    </svg>
  );
}

function Step({ n, title, line }: { n: number; title: string; line: string }) {
  return (
    <div>
      <span className="font-mono text-[12px] text-text-subtle">0{n}</span>
      <h3 className="mt-2 text-[20px] leading-[26px] font-semibold tracking-[-0.01em]">{title}</h3>
      <p className="mt-1 text-[15px] leading-[22px] text-text-muted">{line}</p>
    </div>
  );
}

function Fact({ title, line }: { title: string; line: string }) {
  return (
    <div>
      <h3 className="text-[15px] leading-[22px] font-medium">{title}</h3>
      <p className="mt-1 text-[15px] leading-[22px] text-pretty text-text-muted">{line}</p>
    </div>
  );
}

function Question({ q, children }: { q: string; children: React.ReactNode }) {
  return (
    <details className="group py-4">
      <summary className="flex cursor-pointer list-none items-center justify-between gap-4 text-[15px] font-medium [&::-webkit-details-marker]:hidden">
        {q}
        <span className="text-text-subtle transition-transform duration-150 group-open:rotate-45">+</span>
      </summary>
      <p className="mt-2 text-[15px] leading-[22px] text-pretty text-text-muted">{children}</p>
    </details>
  );
}
