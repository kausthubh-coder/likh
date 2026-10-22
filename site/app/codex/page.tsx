import type { Metadata } from "next";
import Link from "next/link";
import { Bars } from "../bars";

export const metadata: Metadata = {
  title: "Set up Codex · likh",
  description:
    "likh transcribes through ChatGPT with the sign-in Codex saves. Install it, sign in once, and you're done.",
};

/** Where OpenAI sends people for the desktop app that includes Codex (learn.chatgpt.com, Quickstart). */
const WINDOWS_APP = "https://get.microsoft.com/installer/download/9PLM9XGG6VKS?cid=website_cta_psi";
const MAC_APP = "https://persistent.oaistatic.com/codex-app-prod/Codex.dmg";
const PRICING = "https://learn.chatgpt.com/docs/pricing";

export default function CodexSetup() {
  return (
    <div className="mx-auto w-full max-w-[720px] px-5 sm:px-8">
      <header className="flex h-16 items-center">
        <Link href="/" className="flex items-center gap-2 text-[15px] font-semibold tracking-[-0.02em]">
          <Bars className="h-4" />
          likh
        </Link>
      </header>

      <main className="pt-12 sm:pt-20">
        <h1 className="text-[34px] leading-[40px] font-semibold tracking-[-0.03em] text-balance sm:text-[44px] sm:leading-[48px]">
          Set up Codex
        </h1>
        <p className="mt-4 text-[17px] leading-[26px] text-pretty text-text-muted">
          likh doesn&apos;t have an account of its own. It transcribes through ChatGPT using the sign-in Codex saves on
          your computer. Install it and sign in once. If the sign-in expires, open Codex again to refresh it.
        </p>

        <ol className="mt-12 space-y-10">
          <Step n={1} title="Get the ChatGPT desktop app">
            <p>Codex lives inside it. It&apos;s free from OpenAI.</p>
            <div className="mt-4 flex flex-wrap gap-3">
              <Button href={WINDOWS_APP}>For Windows</Button>
              <Button href={MAC_APP} quiet>
                For Mac
              </Button>
            </div>
            <p className="mt-4">Or from a terminal on Windows:</p>
            <Code>winget install 9PLM9XGG6VKS -s msstore</Code>
          </Step>

          <Step n={2} title="Open it and sign in">
            <p>
              Use your ChatGPT account. The app saves the sign-in to <InlineCode>.codex\auth.json</InlineCode> in your
              user folder, which is the file likh reads.
            </p>
            <p className="mt-4">
              Already use the Codex CLI? <InlineCode>codex login</InlineCode> saves the same file, so that works too.
            </p>
          </Step>

          <Step n={3} title="Go back to likh">
            <p>
              The setup window checks every 2 seconds and moves on by itself. If likh is already running, hold Ctrl +
              Space and talk.
            </p>
          </Step>
        </ol>

        <section id="plans" className="mt-20 scroll-mt-8">
          <h2 className="text-[22px] leading-[28px] font-semibold tracking-[-0.02em]">Which plans work</h2>
          <div className="mt-4 space-y-4 text-[15px] leading-[22px] text-pretty text-text-muted">
            <p>
              OpenAI includes Codex with Free, Go, Plus, Pro, Business, Enterprise and Edu. likh has dictated with a
              ChatGPT Free account (tested 21 October 2026) and with paid ones. Go is untested.
            </p>
            <p>
              Clean up is a bonus. It asks ChatGPT to tidy each dictation, and that counts against your plan&apos;s
              Codex allowance. On Free the allowance runs out for the month; likh then pastes what it heard, and
              dictation keeps working.
            </p>
            <p>
              likh itself is free. OpenAI controls access and limits for transcription. Its published desktop Voice
              pricing doesn&apos;t establish how likh&apos;s dictation requests count toward usage, so we can&apos;t
              promise unlimited transcription or quote a per-minute rate. See{" "}
              <a href={PRICING} className="text-text underline-offset-4 hover:underline">
                OpenAI&apos;s pricing
              </a>
              .
            </p>
          </div>
        </section>

        <section className="mt-16">
          <h2 className="text-[22px] leading-[28px] font-semibold tracking-[-0.02em]">If it doesn&apos;t connect</h2>
          <dl className="mt-4 divide-y divide-elevated text-[15px] leading-[22px]">
            <Fix problem="“Your Codex sign-in expired”">
              Open the ChatGPT app once. It refreshes the sign-in, and likh picks it up.
            </Fix>
            <Fix problem="“ChatGPT turned down the sign-in”">
              Sign in to Codex again and retry. This message alone doesn&apos;t tell us whether the problem is the
              sign-in, account permissions or plan access.
            </Fix>
            <Fix problem="likh still says Codex isn't installed">
              Sign in at least once after installing: the file only appears then. If you set{" "}
              <InlineCode>CODEX_HOME</InlineCode>, likh looks there instead.
            </Fix>
          </dl>
          <p className="mt-6 text-[13px] leading-[18px] text-text-subtle">
            likh only ever reads the sign-in. It never changes it, refreshes it, or sends it anywhere but ChatGPT.
          </p>
        </section>
      </main>

      <footer className="mt-24 flex justify-between py-10 text-[13px] text-text-subtle">
        <Link href="/" className="hover:text-text">
          ← likh
        </Link>
        <span>Not affiliated with OpenAI</span>
      </footer>
    </div>
  );
}

function Step({ n, title, children }: { n: number; title: string; children: React.ReactNode }) {
  return (
    <li className="grid grid-cols-[32px_1fr] gap-x-2">
      <span className="font-mono text-[12px] leading-[26px] text-text-subtle">0{n}</span>
      <div>
        <h2 className="text-[20px] leading-[26px] font-semibold tracking-[-0.01em]">{title}</h2>
        <div className="mt-2 text-[15px] leading-[22px] text-pretty text-text-muted">{children}</div>
      </div>
    </li>
  );
}

function Button({ href, quiet = false, children }: { href: string; quiet?: boolean; children: React.ReactNode }) {
  const look = quiet ? "bg-surface text-text hover:bg-elevated" : "bg-accent text-on-accent hover:bg-accent/90";
  return (
    <a
      href={href}
      className={`inline-flex items-center rounded-[10px] px-4 py-2.5 text-[15px] font-medium transition-transform duration-100 ease-(--ease-snappy) active:scale-[0.97] ${look}`}
    >
      {children}
    </a>
  );
}

function Code({ children }: { children: string }) {
  return (
    <pre className="mt-2 overflow-x-auto rounded-[10px] bg-surface px-4 py-3 font-mono text-[13px] leading-[20px] text-text">
      <code>{children}</code>
    </pre>
  );
}

function InlineCode({ children }: { children: string }) {
  return <code className="rounded-[6px] bg-surface px-1.5 py-0.5 font-mono text-[13px] text-text">{children}</code>;
}

function Fix({ problem, children }: { problem: string; children: React.ReactNode }) {
  return (
    <div className="py-4">
      <dt className="font-medium text-text">{problem}</dt>
      <dd className="mt-1 text-pretty text-text-muted">{children}</dd>
    </div>
  );
}
