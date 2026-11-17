import { useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';

/** `--bg` in tokens.css, so the trace sits on the same black as the rest of the app. */
const BACKGROUND = '#0d0e12';
/** How much of the previous frame stays visible, for a soft trail. */
const TRAIL_FADE = 'rgba(13, 14, 18, 0.35)';
/** Pulse goes once around the colour wheel in this time. */
const COLOR_CYCLE_SECONDS = 20;
/** Hue offsets of Pulse's three stops, from left to right. */
const GRADIENT_HUES = [0, 55, 120];

type Theme = 'white' | 'red' | 'blue' | 'pulse';

// The first is the default. Pulse has no single colour.
const THEMES: { theme: Theme; label: string; color?: string }[] = [
  { theme: 'white', label: 'White', color: '#ffffff' },
  { theme: 'red', label: 'Red', color: '#ff4040' },
  { theme: 'blue', label: 'Blue', color: '#5c9dff' },
  { theme: 'pulse', label: 'Pulse' },
];

const wheel = (hue: number) => `hsl(${hue % 360} 100% 68%)`;

interface WaveformProps {
  analyser: AnalyserNode | null;
  /** Where the theme picker goes, in the visualizer's top bar. */
  topSlot: HTMLElement | null;
}

/** Draws the audio being played as a live oscilloscope line. */
export function Waveform({ analyser, topSlot }: WaveformProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  // Always opens on the default theme.
  const [themeIndex, setThemeIndex] = useState(0);
  const themeRef = useRef(THEMES[themeIndex]!);
  themeRef.current = THEMES[themeIndex]!;

  const stepTheme = (step: number) => setThemeIndex((themeIndex + step + THEMES.length) % THEMES.length);

  useEffect(() => {
    const canvas = canvasRef.current;
    const context = canvas?.getContext('2d');
    if (!canvas || !context) return;

    const resize = () => {
      const scale = window.devicePixelRatio;
      canvas.width = Math.round(canvas.clientWidth * scale);
      canvas.height = Math.round(canvas.clientHeight * scale);
      context.fillStyle = BACKGROUND;
      context.fillRect(0, 0, canvas.width, canvas.height);
    };
    const observer = new ResizeObserver(resize);
    observer.observe(canvas);
    resize();

    const samples = new Float32Array(analyser?.fftSize ?? 2048);
    let frame = 0;
    const draw = (now: number) => {
      frame = requestAnimationFrame(draw);
      const { width, height } = canvas;
      if (analyser) {
        analyser.getFloatTimeDomainData(samples);
      } else {
        samples.fill(0);
      }

      context.fillStyle = TRAIL_FADE;
      context.fillRect(0, 0, width, height);

      // Start at a rising zero crossing so a steady sound holds still instead of jittering.
      const visible = samples.length / 2;
      let start = 0;
      for (let i = 1; i < visible; i++) {
        if (samples[i - 1]! < 0 && samples[i]! >= 0) {
          start = i;
          break;
        }
      }

      const middle = height / 2;
      const amplitude = height * 0.42;
      context.beginPath();
      for (let i = 0; i < visible; i++) {
        const x = (i / (visible - 1)) * width;
        const y = middle - samples[start + i]! * amplitude;
        if (i === 0) context.moveTo(x, y);
        else context.lineTo(x, y);
      }
      const theme = themeRef.current;
      context.lineJoin = 'round';
      context.lineWidth = 3 * window.devicePixelRatio;
      if (theme.theme === 'pulse') {
        const hue = (now / 1000 / COLOR_CYCLE_SECONDS) * 360;
        const gradient = context.createLinearGradient(0, 0, width, 0);
        GRADIENT_HUES.forEach((offset, index) => {
          gradient.addColorStop(index / (GRADIENT_HUES.length - 1), wheel(hue + offset));
        });
        context.strokeStyle = gradient;
        context.shadowColor = wheel(hue + GRADIENT_HUES[1]!);
      } else {
        context.strokeStyle = theme.color!;
        context.shadowColor = theme.color!;
      }
      context.shadowBlur = 18 * window.devicePixelRatio;
      context.stroke();
      context.shadowBlur = 0;
    };
    draw(performance.now());

    return () => {
      cancelAnimationFrame(frame);
      observer.disconnect();
    };
  }, [analyser]);

  return (
    <>
      <canvas ref={canvasRef} className="visualizer-canvas" />
      {topSlot &&
        createPortal(
          <div className="milkdrop-presets">
            <button aria-label="Previous style" onClick={() => stepTheme(-1)}>
              ‹
            </button>
            <span className="milkdrop-preset-name">{THEMES[themeIndex]!.label}</span>
            <button aria-label="Next style" onClick={() => stepTheme(1)}>
              ›
            </button>
          </div>,
          topSlot,
        )}
    </>
  );
}
