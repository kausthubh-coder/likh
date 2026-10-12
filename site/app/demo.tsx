"use client";

// The hero demo: a dictation into a bright document, looping. Hold the keys,
// the pill listens, let go, it thinks, the sentence lands at the caret.
// It only runs while on screen, and holds one still frame for reduced motion.

import { useEffect, useRef, useState, useSyncExternalStore } from "react";
import { Pill, type PillState } from "./pill";

const TAKES = [
  { app: "Mail · Reply to Priya", before: "Thanks for the notes. ", said: "Also, remind me to send the deck to you tomorrow at nine." },
  { app: "Slack · #launch", before: "Plan for this week: ", said: "ship the beta to the first hundred testers on Friday, then we decide on pricing." },
  { app: "Notes · Groceries", before: "Eggs, oat milk, ", said: "coffee beans, the good bread, and something for dinner on Sunday." },
];

type Phase = { pill: PillState; keys: boolean; landed: boolean };
const SCRIPT: [number, Phase][] = [
  [0, { pill: "hidden", keys: false, landed: false }],
  [900, { pill: "listening", keys: true, landed: false }],
  [3600, { pill: "thinking", keys: false, landed: false }],
  [4500, { pill: "done", keys: false, landed: true }],
  [4800, { pill: "hidden", keys: false, landed: true }],
];
const LOOP = 8200;
/** What reduced motion shows: one still frame with everything visible. */
const STILL: Phase = { pill: "listening", keys: true, landed: true };

const REDUCE = "(prefers-reduced-motion: reduce)";
function useReducedMotion() {
  return useSyncExternalStore(
    (changed) => {
      const query = matchMedia(REDUCE);
      query.addEventListener("change", changed);
      return () => query.removeEventListener("change", changed);
    },
    () => matchMedia(REDUCE).matches,
    () => false,
  );
}

export function Demo() {
  const root = useRef<HTMLDivElement>(null);
  const [take, setTake] = useState(0);
  const [playing, setPlaying] = useState<Phase>(SCRIPT[0][1]);
  const still = useReducedMotion();
  const phase = still ? STILL : playing;

  useEffect(() => {
    if (still) return;
    let timers: number[] = [];
    const play = (n: number) => {
      setTake(n % TAKES.length);
      timers = SCRIPT.map(([at, next]) => window.setTimeout(() => setPlaying(next), at));
      timers.push(window.setTimeout(() => play(n + 1), LOOP));
    };
    const stop = () => timers.forEach(clearTimeout);
    // Nothing runs while the demo is scrolled away.
    const seen = new IntersectionObserver(([entry]) => {
      stop();
      if (entry.isIntersecting) play(0);
      else setPlaying(SCRIPT[0][1]);
    });
    if (root.current) seen.observe(root.current);
    return () => {
      stop();
      seen.disconnect();
    };
  }, [still]);

  const { app, before, said } = TAKES[take];
  return (
    <div ref={root} className="relative h-[340px] overflow-hidden rounded-2xl bg-paper text-ink sm:h-[380px]">
      <div className="flex items-center gap-2 px-5 pt-4 sm:px-8">
        <span className="size-2.5 rounded-full bg-ink/15" />
        <span className="size-2.5 rounded-full bg-ink/15" />
        <span className="size-2.5 rounded-full bg-ink/15" />
        <span className="ml-3 truncate text-[13px] text-ink-muted">{app}</span>
      </div>
      <p className="max-w-[46ch] px-5 pt-8 text-[17px] leading-[28px] sm:px-8">
        {before}
        <span
          className="transition-[opacity,background-color] duration-500 ease-(--ease-snappy)"
          style={{ opacity: phase.landed ? 1 : 0, backgroundColor: phase.landed && phase.pill !== "hidden" ? "oklch(0.78 0.11 45 / 0.25)" : "transparent" }}
        >
          {phase.landed ? said : ""}
        </span>
        <span className="ml-px inline-block h-5 w-0.5 animate-[caret_1s_steps(1)_infinite] bg-ink align-[-4px]" />
      </p>

      <div className="absolute top-4 right-5 flex items-center gap-1.5 sm:right-8" aria-hidden="true">
        <Key down={phase.keys}>Ctrl</Key>
        <span className="text-[12px] text-ink-muted">+</span>
        <Key down={phase.keys}>Win</Key>
      </div>

      <div className="absolute inset-x-0 bottom-6 flex justify-center">
        <Pill state={phase.pill} />
      </div>
    </div>
  );
}

function Key({ down, children }: { down: boolean; children: string }) {
  return (
    <span
      className={`rounded-md px-2 py-1 font-mono text-[12px] leading-4 shadow-(--shadow-key) transition-[background-color,color,translate] duration-100 ${
        down ? "translate-y-px bg-accent text-on-accent" : "bg-surface text-text"
      }`}
    >
      {children}
    </span>
  );
}
