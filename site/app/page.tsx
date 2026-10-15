import Link from "next/link";
import { Bars } from "./bars";
import { Demo } from "./demo";
import { FlockMark } from "./flock-mark";

const REPO = "https://github.com/kausthubh-coder/likh";
/** The published build. Bump with each release; the page reads everything else from here. */
const RELEASE = {
  version: "0.1.0-alpha.9",
  windows: `${REPO}/releases/download/v0.1.0-alpha.9/likh-setup-windows-x64.exe`,
  size: "5.0 MB",
};

/** Measured on the release build, idle, Windows 11 (docs/plan.md, Measured so far). */
const STATS = [
  { value: "14 MB", label: "of memory while it waits" },
  { value: "0%", label: "CPU while it waits" },
  { value: RELEASE.size, label: "the whole download" },
  { value: "0", label: "browser engines inside" },
];

const FEATURES = [
  { title: "Hold or tap", line: "Hold to talk, or tap to go hands-free. It stops when you go quiet." },
  { title: "Voice commands", line: "“New line”, “new bullet”, “press enter”, “scratch that”." },
  { title: "Clean up", line: "Drops the ums, keeps your “actually, 3”, and turns spoken lists into lists." },
  { title: "Your own words", line: "Project names and people, spelled your way every time." },
  { title: "Paste it again", line: "Alt + Shift + Z pastes your last dictation anywhere." },
  { title: "History", line: "Every dictation, kept on your computer. Click to copy." },
  { title: "Your shortcut", line: "Press the keys you want, or use a mouse button." },
  { title: "Two free engines", line: "Your ChatGPT plan, or a free Groq key." },
  { title: "Keeps itself current", line: "Starts with Windows and updates when you say so." },
];

/**
 * Memory while idle, in MB. likh and Handy we measured ourselves on Windows 11
 * (Handy: a build of Handy 0.x, all its processes together). Wispr Flow is what
 * reviewers report for its Windows app; linked in the footnote.
 */
const MEMORY = [
  { name: "likh", mb: 14, note: "measured" },
  { name: "Handy", mb: 140, note: "measured" },
  { name: "Wispr Flow", mb: 800, note: "reported" },
];

const COMPARE: { label: string; likh: string; wispr: string; handy: string }[] = [
  { label: "Price", likh: "Free", wispr: "Free to 2,000 words a week, then $12–15/mo", handy: "Free" },
  {
    label: "Transcribes with",
    likh: "ChatGPT",
    wispr: "Wispr's servers",
    handy: "A model on your computer",
  },
  { label: "Download", likh: RELEASE.size, wispr: "An Electron app", handy: "App, plus a 0.5–1.6 GB model" },
  { label: "Open source", likh: "Yes, MIT", wispr: "No", handy: "Yes, MIT" },
];

export default function Home() {
  return (
    <div className="mx-auto w-full max-w-[1080px] px-5 sm:px-8">
      <header className="flex h-16 items-center justify-between">
        <a href="#" className="flex items-center gap-2 text-[15px] font-semibold tracking-[-0.02em]">
          <Bars className="h-4" />
          likh
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
            Hold Ctrl + Space, talk, let go. Your words land wherever you&apos;re typing.
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
        </section>

        <section className="mt-24 sm:mt-32">
          <h2 className="text-[28px] leading-[34px] font-semibold tracking-[-0.02em]">What it does</h2>
          <div className="mt-8 grid gap-x-8 gap-y-6 sm:grid-cols-3">
            {FEATURES.map((feature) => (
              <div key={feature.title}>
                <h3 className="text-[15px] leading-[22px] font-medium">{feature.title}</h3>
                <p className="mt-1 text-[15px] leading-[22px] text-pretty text-text-muted">{feature.line}</p>
              </div>
            ))}
          </div>
        </section>

        <Compare />

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
              <span className="text-text">1.</span>{" "}
              <Link href="/codex" className="text-text underline-offset-4 hover:underline">
                Set up Codex
              </Link>{" "}
              and sign in once.
            </p>
            <p>
              <span className="text-text">2.</span> Run the installer. It isn&apos;t signed yet, so Windows asks first: More info, then
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
              likh is free and MIT licensed. OpenAI controls transcription access and limits; we haven&apos;t confirmed
              how these requests count toward your usage.{" "}
              <Link href="/codex#plans" className="text-text underline-offset-4 hover:underline">
                Which plans work
              </Link>
              .
            </Question>
            <Question q="Is this an official OpenAI app?">
              No. likh uses the transcription endpoint the Codex desktop app uses for its own dictation. OpenAI can change it
              at any time; if that happens, likh ships an update or stops working.
            </Question>
            <Question q="How long can I talk?">
              Up to 30 minutes at a time. Past three, likh sends what you&apos;ve said while you keep talking, so the wait
              when you stop stays short. The pill counts down your last minute.
            </Question>
            <Question q="Where does my voice go?">
              Straight to ChatGPT through your Codex sign-in, or to Groq if you pick it, and nowhere else. Clean up and your
              dictionary send the text, never the audio, to ChatGPT. History stays on your computer, and you can turn it off or
              clear it.
            </Question>
          </div>
        </section>
      </main>

      <footer className="mt-24 flex flex-col gap-2 py-10 text-[13px] text-text-subtle sm:mt-32 sm:flex-row sm:justify-between">
        <span>likh · MIT · Sanskrit for “write”</span>
        <a href={REPO} className="hover:text-text">
          Source on GitHub
        </a>
      </footer>
    </div>
  );
}

function Compare() {
  const most = Math.max(...MEMORY.map((row) => row.mb));
  return (
    <section className="mt-24 sm:mt-32">
      <h2 className="text-[28px] leading-[34px] font-semibold tracking-[-0.02em]">How it compares</h2>
      <figure className="mt-8 max-w-[720px]">
        <figcaption className="text-[13px] text-text-subtle">Memory while it waits, on Windows</figcaption>
        <div className="mt-4 space-y-3">
          {MEMORY.map((row) => (
            <div key={row.name} className="grid grid-cols-[96px_1fr_auto] items-center gap-4">
              <span className={`text-[13px] ${row.name === "likh" ? "font-medium text-text" : "text-text-muted"}`}>
                {row.name}
              </span>
              <span className="h-2 rounded-full bg-surface">
                <span
                  className={`block h-full rounded-full ${row.name === "likh" ? "bg-accent" : "bg-text-subtle"}`}
                  style={{ width: `${Math.max(1.5, (row.mb / most) * 100)}%` }}
                />
              </span>
              <span className="w-[132px] text-right text-[13px] whitespace-nowrap tabular-nums text-text-muted">
                ~{row.mb} MB <span className="text-text-subtle">{row.note}</span>
              </span>
            </div>
          ))}
        </div>
      </figure>

      <div className="mt-12 overflow-x-auto">
        <table className="w-full min-w-[640px] text-left text-[15px] leading-[22px]">
          <thead>
            <tr className="text-[13px] text-text-subtle">
              <th className="w-[20%] pb-3 font-normal" />
              <th className="pb-3 font-medium text-text">likh</th>
              <th className="pb-3 font-normal">Wispr Flow</th>
              <th className="pb-3 font-normal">Handy</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-elevated">
            {COMPARE.map((row) => (
              <tr key={row.label} className="align-top">
                <td className="py-3 pr-4 text-[13px] text-text-subtle">{row.label}</td>
                <td className="py-3 pr-4 text-text">{row.likh}</td>
                <td className="py-3 pr-4 text-text-muted">{row.wispr}</td>
                <td className="py-3 text-text-muted">{row.handy}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <p className="mt-6 max-w-[60em] text-[13px] leading-[18px] text-text-subtle">
        Prices from{" "}
        <a href="https://wisprflow.ai/pricing" className="underline-offset-4 hover:underline">
          Wispr Flow&apos;s pricing page
        </a>{" "}
        and model sizes from{" "}
        <a href="https://github.com/cjpais/Handy" className="underline-offset-4 hover:underline">
          Handy&apos;s README
        </a>
        , October 2026. Wispr Flow&apos;s memory is what reviewers report for its Windows app (
        <a href="https://spokenly.app/blog/wispr-flow-review" className="underline-offset-4 hover:underline">
          Spokenly
        </a>
        ,{" "}
        <a
          href="https://www.getvoibe.com/resources/best-wispr-flow-alternatives-windows/"
          className="underline-offset-4 hover:underline"
        >
          Voibe
        </a>
        ); likh we measured, and Handy from a build of its source. Handy works offline, which likh doesn&apos;t.
      </p>
    </section>
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

function Step({ n, title, line }: { n: number; title: string; line: string }) {
  return (
    <div>
      <span className="font-mono text-[12px] text-text-subtle">0{n}</span>
      <h3 className="mt-2 text-[20px] leading-[26px] font-semibold tracking-[-0.01em]">{title}</h3>
      <p className="mt-1 text-[15px] leading-[22px] text-text-muted">{line}</p>
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
