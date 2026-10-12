import { Demo } from "./demo";
import { FlockMark } from "./flock-mark";
import { Pill } from "./pill";

const REPO = "https://github.com/kausthubh-coder/murmur";
/** Set to the GitHub Releases download once v0.1.0 is published; until then the button says so. */
const DOWNLOAD: string | null = null;

export default function Home() {
  return (
    <div className="mx-auto w-full max-w-[1080px] px-5 sm:px-8">
      <header className="flex h-16 items-center justify-between">
        <a href="#" className="flex items-center gap-2 text-[15px] font-semibold tracking-[-0.02em]">
          <Bars className="h-4" />
          murmur
        </a>
        <a href={REPO} className="-mr-3 rounded-[10px] px-3 py-2 text-[13px] font-medium text-text-muted hover:text-text">
          GitHub
        </a>
      </header>

      <main>
        <section className="flex flex-col items-center pt-12 text-center sm:pt-20">
          <FlockMark />
          <h1 className="mt-6 text-[44px] leading-[48px] font-semibold tracking-[-0.03em] text-balance sm:text-[72px] sm:leading-[76px]">
            Talk. It types.
          </h1>
          <p className="mt-5 max-w-[34em] text-[17px] leading-[26px] text-pretty text-text-muted">
            Hold Ctrl + Space, say it, let go. Your words land wherever you&apos;re typing, transcribed with the ChatGPT plan you
            already have.
          </p>
          <div className="mt-8 flex flex-col items-center gap-3">
            <a
              href={DOWNLOAD ?? REPO}
              className="rounded-[10px] bg-accent px-5 py-3 text-[15px] font-medium text-on-accent transition-transform duration-100 ease-(--ease-snappy) hover:bg-accent/90 active:scale-[0.97]"
            >
              {DOWNLOAD ? "Download for Windows" : "Follow the first release on GitHub"}
            </a>
            <span className="text-[13px] text-text-subtle">Free and open source · Windows 10 and 11 · macOS in alpha</span>
          </div>
        </section>

        <section className="mt-16 sm:mt-24">
          <Demo />
        </section>

        <section className="mt-24 grid gap-10 sm:mt-32 sm:grid-cols-3 sm:gap-8">
          <Step n={1} title="Hold Ctrl + Space">
            Anywhere: email, Slack, your editor, a terminal. murmur never takes focus, so your cursor stays put.
          </Step>
          <Step n={2} title="Say it">
            Talk the way you would to a person. The pill&apos;s bars move with your voice so you know it&apos;s listening.
          </Step>
          <Step n={3} title="Let go">
            The text pastes at the cursor in about a second. Esc while you talk throws it away.
          </Step>
        </section>

        <section className="mt-24 sm:mt-32">
          <h2 className="max-w-[20em] text-[28px] leading-[34px] font-semibold tracking-[-0.02em] text-balance sm:text-[34px] sm:leading-[40px]">
            Built to stay out of the way, all day.
          </h2>
          <dl className="mt-10 grid gap-x-12 gap-y-8 sm:grid-cols-2">
            <Fact title="Costs nothing to leave running">
              Rust, drawn on the CPU, no browser inside. While you aren&apos;t talking it sits at about 11 MB of memory and no
              CPU at all.
            </Fact>
            <Fact title="Uses the plan you already pay for">
              murmur signs in through Codex and transcribes with your ChatGPT plan. No new account, no API key, nothing extra to
              pay.
            </Fact>
            <Fact title="Only one place sees your voice">
              Audio goes straight to ChatGPT and nowhere else. No murmur account, no analytics. The mic is open only while you
              hold the keys.
            </Fact>
            <Fact title="One quiet pill">
              A small capsule at the bottom of the screen. It listens, thinks, and gets out of the way. Errors say what happened
              and how to fix it.
            </Fact>
          </dl>
        </section>

        <section className="mt-24 sm:mt-32">
          <div className="grid grid-cols-2 gap-y-10 sm:grid-cols-4">
            <State label="Listening">
              <Pill state="listening" />
            </State>
            <State label="Thinking">
              <Pill state="thinking" />
            </State>
            <State label="A minute left">
              <Pill state="warning" seconds={55} />
            </State>
            <State label="Done">
              <span className="text-[13px] text-text-muted">The text is the feedback.</span>
            </State>
          </div>
        </section>

        <section className="mt-24 max-w-[44em] sm:mt-32">
          <h2 className="text-[28px] leading-[34px] font-semibold tracking-[-0.02em]">Questions</h2>
          <div className="mt-6 divide-y divide-elevated">
            <Question q="Is it free?">
              Yes. murmur is MIT licensed. Transcription runs on the ChatGPT plan you already have through Codex, so there is
              nothing to buy.
            </Question>
            <Question q="Do I need Codex?">
              You need Codex installed and signed in once. murmur reads that sign-in; it never changes it, and Codex doesn&apos;t
              have to be running.
            </Question>
            <Question q="Is this an official OpenAI app?">
              No. murmur uses the same transcription endpoint the Codex desktop app uses for its dictation. OpenAI can change it at
              any time; if that happens, murmur ships an update or stops working.
            </Question>
            <Question q="How long can I talk?">
              Up to ten minutes per dictation, the same limit as Codex. The pill counts down your last minute.
            </Question>
            <Question q="What about macOS?">
              It builds and runs, but nobody on the team has a Mac to test on yet, so it ships labelled alpha.
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

function Step({ n, title, children }: { n: number; title: string; children: React.ReactNode }) {
  return (
    <div>
      <span className="font-mono text-[12px] text-text-subtle">0{n}</span>
      <h3 className="mt-2 text-[20px] leading-[26px] font-semibold tracking-[-0.01em]">{title}</h3>
      <p className="mt-2 text-[15px] leading-[22px] text-pretty text-text-muted">{children}</p>
    </div>
  );
}

function Fact({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div>
      <dt className="text-[15px] leading-[22px] font-medium">{title}</dt>
      <dd className="mt-1 text-[15px] leading-[22px] text-pretty text-text-muted">{children}</dd>
    </div>
  );
}

function State({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-col items-center gap-4">
      <div className="flex h-14 items-center">{children}</div>
      <span className="text-[13px] text-text-subtle">{label}</span>
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
