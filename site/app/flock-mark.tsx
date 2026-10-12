"use client";

// The logo arriving as a murmuration: dots sweep in from the left and land
// on the five bars, once, when the page opens. Then it is a plain SVG and
// draws nothing. The same choreography as the app's welcome screen
// (app/src/onboarding/flock.rs), shrunk to a page mark.

import { useEffect, useRef, useState } from "react";

const HEIGHTS = [0.45, 0.85, 0.55, 0.85, 0.45];
const SIZE = { w: 120, h: 72 };
const UNIT = 1.6; // one logo unit in CSS pixels (the bars sit in a 64 x 40 box)
const BIRDS = 60;
const FLY = 0.9;
const LAND_FROM = 0.45;
const SWEEP = 0.3;
const DURATION = LAND_FROM + SWEEP + 0.1 + 0.64;

function noise(i: number, salt: number) {
  const x = Math.sin(i * 12.9898 + salt * 78.233) * 43758.5453;
  return x - Math.floor(x);
}

/** Closed-form critically damped spring released at 0 towards 1 (spring.default, 0.4s). */
function spring(t: number) {
  if (t <= 0) return 0;
  const w = (2 * Math.PI) / 0.4;
  return 1 - Math.exp(-w * t) * (1 + w * t);
}

function slots() {
  const out: [number, number][] = [];
  const r = 2 * UNIT;
  HEIGHTS.forEach((share, i) => {
    const x = (17 + 8 * i - 32) * UNIT;
    const half = (22 * share * UNIT) / 2 - r;
    const n = Math.ceil((2 * half) / (1.2 * r)) + 1;
    for (let k = 0; k < n; k++) out.push([x, -half + (2 * half * k) / Math.max(1, n - 1)]);
  });
  return out;
}

export function FlockMark() {
  const canvas = useRef<HTMLCanvasElement>(null);
  const [landed, setLanded] = useState(false);

  useEffect(() => {
    const el = canvas.current;
    const ctx = el?.getContext("2d");
    if (!el || !ctx || matchMedia("(prefers-reduced-motion: reduce)").matches) {
      setLanded(true);
      return;
    }
    const dpr = devicePixelRatio || 1;
    el.width = SIZE.w * dpr;
    el.height = SIZE.h * dpr;
    const spots = slots();
    const left = Math.min(...spots.map((s) => s[0]));
    const span = Math.max(...spots.map((s) => s[0])) - left;
    const accent = getComputedStyle(el).getPropertyValue("--color-accent");
    const muted = getComputedStyle(el).getPropertyValue("--color-text-muted");
    const start = performance.now();
    let raf = 0;
    const frame = (now: number) => {
      const t = (now - start) / 1000;
      ctx.setTransform(dpr, 0, 0, dpr, SIZE.w * dpr * 0.5, SIZE.h * dpr * 0.5);
      ctx.clearRect(-SIZE.w, -SIZE.h, SIZE.w * 2, SIZE.h * 2);
      for (let i = 0; i < BIRDS; i++) {
        const [sx, sy] = spots[i % spots.length];
        const u = Math.min(1, Math.max(0, (t - 0.2 * noise(i, 1)) / FLY));
        const e = 1 - Math.pow(1 - u, 3);
        // The flock's path: from off the left, up and round, into the middle.
        const v = 1 - e;
        const px = 3 * v * v * e * (-0.1 * SIZE.w) + 3 * v * e * e * (0.5 * SIZE.w) + v * v * v * (-0.9 * SIZE.w);
        const py = 3 * v * v * e * (-0.7 * SIZE.h) + 3 * v * e * e * (-0.25 * SIZE.h) + v * v * v * (0.3 * SIZE.h);
        const a = Math.PI * 2 * noise(i, 2);
        const reach = Math.sqrt(noise(i, 3)) * (1 - 0.5 * e);
        const fx = px + Math.cos(a) * reach * SIZE.w * 0.25;
        const fy = py + Math.sin(a) * reach * SIZE.h * 0.2;
        const land = spring(t - (LAND_FROM + (SWEEP * (sx - left)) / span + 0.1 * noise(i, 5)));
        ctx.globalAlpha = 0.6 + 0.4 * land;
        ctx.fillStyle = land > 0.5 ? accent : muted;
        ctx.beginPath();
        ctx.arc(fx + (sx - fx) * land, fy + (sy - fy) * land, 1.2 + (2 * UNIT - 1.2) * land, 0, Math.PI * 2);
        ctx.fill();
      }
      if (t < DURATION) raf = requestAnimationFrame(frame);
      else setLanded(true);
    };
    raf = requestAnimationFrame(frame);
    return () => cancelAnimationFrame(raf);
  }, []);

  return (
    <div className="relative" style={{ width: SIZE.w, height: SIZE.h }} aria-hidden="true">
      <canvas ref={canvas} className="absolute inset-0 size-full" style={{ opacity: landed ? 0 : 1 }} />
      <svg
        viewBox="-60 -36 120 72"
        className="absolute inset-0 size-full transition-opacity duration-150"
        style={{ opacity: landed ? 1 : 0 }}
      >
        {HEIGHTS.map((share, i) => {
          const h = 22 * share * UNIT;
          return <rect key={i} x={(15 + 8 * i - 32) * UNIT} y={-h / 2} width={4 * UNIT} height={h} rx={2 * UNIT} className="fill-accent" />;
        })}
      </svg>
    </div>
  );
}
