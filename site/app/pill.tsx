"use client";

// The pill, as the app draws it (docs/brand/BRAND.md, Pill states). Sizes and
// insets match app/src/pill/mod.rs: seven 4px bars, 3px apart, 15px from the left.

import { useEffect, useRef } from "react";

export type PillState = "hidden" | "listening" | "thinking" | "warning" | "error" | "done";

const SIZES: Record<PillState, [number, number]> = {
  hidden: [40, 8],
  listening: [76, 36],
  thinking: [76, 36],
  warning: [100, 36],
  error: [300, 52],
  done: [40, 8],
};

const BARS = 7;

/** Stand-in for mic levels: syllables, louder and softer phrases, per-bar grain. */
function voiceLevel(t: number, i: number) {
  const syllable = Math.pow(Math.abs(Math.sin(t * 5.4)), 0.5);
  const phrase = 0.7 + 0.3 * Math.sin(t * 1.3);
  const grain = 0.6 + 0.4 * Math.abs(Math.sin(t * 13 + i * 2.1) * Math.cos(t * 7.7 + i * 1.3));
  return Math.min(1, 1.8 * syllable * phrase * grain * (1 - Math.abs(i - 3) * 0.1));
}

export function Pill({ state, seconds = 55, error = "" }: { state: PillState; seconds?: number; error?: string }) {
  const bars = useRef<(HTMLSpanElement | null)[]>([]);
  const [w, h] = SIZES[state];
  const moving = state === "listening" || state === "thinking" || state === "warning";

  // Frames only while the bars move; a still pill costs nothing.
  useEffect(() => {
    if (!moving) {
      bars.current.forEach((bar) => bar && (bar.style.height = "4px"));
      return;
    }
    const still = matchMedia("(prefers-reduced-motion: reduce)").matches;
    const levels = new Array(BARS).fill(0);
    let raf = 0;
    let last = performance.now();
    const frame = (now: number) => {
      const dt = Math.min(0.032, (now - last) / 1000);
      last = now;
      const t = now / 1000;
      bars.current.forEach((bar, i) => {
        if (!bar) return;
        const target =
          state === "thinking"
            ? still
              ? 0.3
              : 0.15 + (0.35 * (1 + Math.sin(t * 5.2 - i * 0.7))) / 2
            : still
              ? [0.3, 0.6, 0.45, 0.8, 0.45, 0.6, 0.3][i]
              : voiceLevel(t, i);
        const rate = target > levels[i] ? 0.04 : 0.14; // attack 40ms, release 140ms
        levels[i] += (target - levels[i]) * Math.min(1, dt / rate);
        bar.style.height = `${4 + levels[i] * 16}px`;
      });
      raf = still ? 0 : requestAnimationFrame(frame);
    };
    raf = requestAnimationFrame(frame);
    return () => cancelAnimationFrame(raf);
  }, [moving, state]);

  const voiced = state !== "error";
  return (
    <div
      aria-hidden="true"
      className="relative overflow-hidden bg-surface shadow-(--shadow-pill) transition-[width,height,border-radius,opacity] duration-250 ease-(--ease-snappy)"
      style={{ width: w, height: h, borderRadius: h / 2, opacity: state === "hidden" ? 0 : state === "done" ? 0.7 : 1 }}
    >
      <div
        className="absolute top-2 left-[15px] flex h-5 items-center gap-[3px] transition-opacity duration-150"
        style={{ opacity: voiced ? 1 : 0 }}
      >
        {Array.from({ length: BARS }, (_, i) => (
          <span
            key={i}
            ref={(el) => {
              bars.current[i] = el;
            }}
            className={`block h-1 w-1 rounded-full transition-colors duration-150 ${state === "thinking" ? "bg-text-muted" : "bg-accent"}`}
          />
        ))}
      </div>
      <div
        className="absolute top-1.5 right-1.5 grid size-6 place-items-center rounded-full bg-elevated transition-[opacity,scale] duration-150"
        style={{ opacity: state === "warning" ? 1 : 0, scale: state === "warning" ? 1 : 0.9 }}
      >
        <svg viewBox="0 0 24 24" className="absolute inset-0 -rotate-90" fill="none">
          <circle
            cx="12"
            cy="12"
            r="10.5"
            stroke="var(--color-warning)"
            strokeWidth="2"
            strokeLinecap="round"
            pathLength={100}
            strokeDasharray="100"
            strokeDashoffset={100 - (seconds / 60) * 100}
          />
        </svg>
        <span className="text-[12px] leading-none font-medium tabular-nums">{seconds}</span>
      </div>
      <div
        className="absolute inset-0 flex items-center gap-3 pr-4 pl-4 transition-opacity duration-150"
        style={{ opacity: state === "error" ? 1 : 0 }}
      >
        <svg width="16" height="16" viewBox="0 0 16 16" className="flex-none text-danger" fill="currentColor">
          <path d="M8 1.5a6.5 6.5 0 1 0 0 13 6.5 6.5 0 0 0 0-13Zm-.75 3.25h1.5v4.5h-1.5v-4.5Zm.75 7.1a.9.9 0 1 1 0-1.8.9.9 0 0 1 0 1.8Z" />
        </svg>
        <span className="text-[13px] leading-[18px] font-medium text-pretty">{error}</span>
      </div>
    </div>
  );
}
