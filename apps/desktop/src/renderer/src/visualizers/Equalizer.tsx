import { useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { bandLevel, logBandRanges } from './equalizerBands';

/** Fine enough that neighbouring bass bars do not all read the same bin. */
const FFT_SIZE = 16384;
const BAR_COUNT = 96;
const MIN_HZ = 35;
const MAX_HZ = 16000;
/** Levels from here (an empty bar) to here (a full one), in dB. */
const FLOOR_DB = -76;
const CEILING_DB = -16;
/** Bars jump up and fall back; the peak caps hang on a moment before drifting down. */
const FALL_PER_SECOND = 1.1;
const PEAK_HOLD_SECONDS = 0.6;
const PEAK_FALL_PER_SECOND = 0.35;

type Style = 'mirror' | 'led' | 'fire';

// The first is the default.
const STYLES: { style: Style; label: string }[] = [
  { style: 'mirror', label: 'Mirror' },
  { style: 'led', label: 'LED' },
  { style: 'fire', label: 'Fire' },
];

interface MirroredPalette {
  backgroundEdge: string;
  /** Behind the baseline, where the bars meet their reflection. */
  backgroundMiddle: string;
  /** From the baseline up. */
  bars: string[];
  /** From the baseline down, fading out. */
  reflection: [string, string];
  baseline: string;
  peak: string;
}

/** Styles drawn as bars standing on a baseline above their reflection. */
const MIRRORED_PALETTES: Record<Exclude<Style, 'led'>, MirroredPalette> = {
  mirror: {
    backgroundEdge: '#0c0a1f',
    backgroundMiddle: '#2a1a55',
    bars: ['#7b4dff', '#3f8cff', '#43e6ff', '#b8fff4'],
    reflection: ['rgba(67, 230, 255, 0.45)', 'rgba(123, 77, 255, 0)'],
    baseline: 'rgba(184, 255, 244, 0.6)',
    peak: '#ffffff',
  },
  fire: {
    backgroundEdge: '#050303',
    backgroundMiddle: '#2a0d06',
    bars: ['#b3140c', '#f05a14', '#ffb81f', '#ffe44a', '#fff7a8'],
    reflection: ['rgba(255, 140, 30, 0.45)', 'rgba(179, 20, 12, 0)'],
    baseline: 'rgba(255, 220, 140, 0.6)',
    peak: '#fff8d6',
  },
};

/** The LED style's lights are bigger, so each column covers two bars. */
const LED_COLUMNS = BAR_COUNT / 2;
/** Rows above these fractions of the column light yellow, then red. */
const LED_YELLOW_FROM = 0.6;
const LED_RED_FROM = 0.85;
const LED_COLORS = { green: '#3dff6e', yellow: '#ffd83a', red: '#ff3b30' };
type LedColor = keyof typeof LED_COLORS;

const ledColor = (row: number, rows: number): LedColor => {
  const position = (row + 0.5) / rows;
  return position >= LED_RED_FROM ? 'red' : position >= LED_YELLOW_FROM ? 'yellow' : 'green';
};

interface LedLayout {
  width: number;
  height: number;
  slot: number;
  rows: number;
  rowPitch: number;
  base: number;
  radius: number;
  /** The unlit lights, drawn once per size. */
  background: HTMLCanvasElement;
}

/** The height of `.visualizer-bottom`, in CSS pixels; the lights stop above it. */
const BOTTOM_CONTROLS_HEIGHT = 68;
/** Lamp radius as a fraction of the tighter cell pitch. */
const LED_RADIUS_RATIO = 0.34;

export interface LedMetrics {
  slot: number;
  rows: number;
  rowPitch: number;
  base: number;
  radius: number;
}

/** Cell size for the LED matrix. Lamps stay inside their cells so a dark gap remains. */
export function ledMetrics(width: number, height: number, px: number): LedMetrics {
  const slot = width / LED_COLUMNS;
  const top = height * 0.05;
  const base = height - (BOTTOM_CONTROLS_HEIGHT + 4) * px;
  const rows = Math.max(4, Math.floor((base - top) / slot));
  const rowPitch = (base - top) / rows;
  const radius = Math.min(slot, rowPitch) * LED_RADIUS_RATIO;
  return { slot, rows, rowPitch, base, radius };
}

function ledLayout(width: number, height: number, px: number): LedLayout {
  const { slot, rows, rowPitch, base, radius } = ledMetrics(width, height, px);

  const background = document.createElement('canvas');
  background.width = width;
  background.height = height;
  const context = background.getContext('2d')!;
  context.fillStyle = '#000';
  context.fillRect(0, 0, width, height);
  for (let column = 0; column < LED_COLUMNS; column++) {
    for (let row = 0; row < rows; row++) {
      context.fillStyle = LED_COLORS[ledColor(row, rows)];
      context.globalAlpha = 0.09;
      context.beginPath();
      context.arc((column + 0.5) * slot, base - (row + 0.5) * rowPitch, radius, 0, Math.PI * 2);
      context.fill();
    }
  }
  context.globalAlpha = 1;

  return { width, height, slot, rows, rowPitch, base, radius, background };
}
interface EqualizerProps {
  analyser: AnalyserNode | null;
  /** Where the style picker goes, in the visualizer's top bar. */
  topSlot: HTMLElement | null;
}

/** A classic media-player spectrum: a row of bars with falling peak caps. */
export function Equalizer({ analyser, topSlot }: EqualizerProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  // Always opens on the default style.
  const [styleIndex, setStyleIndex] = useState(0);
  const styleRef = useRef<Style>(STYLES[styleIndex]!.style);
  styleRef.current = STYLES[styleIndex]!.style;

  const stepStyle = (step: number) => setStyleIndex((styleIndex + step + STYLES.length) % STYLES.length);

  useEffect(() => {
    const canvas = canvasRef.current;
    const context = canvas?.getContext('2d');
    if (!canvas || !context) return;

    // An analyser passes its input straight through, so this one can hang off the shared one.
    let tap: AnalyserNode | null = null;
    if (analyser) {
      tap = analyser.context.createAnalyser();
      tap.fftSize = FFT_SIZE;
      tap.smoothingTimeConstant = 0.5;
      analyser.connect(tap);
    }
    const ranges = logBandRanges(analyser?.context.sampleRate ?? 48000, FFT_SIZE, BAR_COUNT, MIN_HZ, MAX_HZ);
    const decibels = new Float32Array(FFT_SIZE / 2);
    const levels = new Float32Array(BAR_COUNT);
    const peaks = Array.from({ length: BAR_COUNT }, () => ({ level: 0, heldFor: 0 }));

    const resize = () => {
      const scale = window.devicePixelRatio;
      canvas.width = Math.round(canvas.clientWidth * scale);
      canvas.height = Math.round(canvas.clientHeight * scale);
    };
    const observer = new ResizeObserver(resize);
    observer.observe(canvas);
    resize();

    let led: LedLayout | null = null;
    let last = performance.now();
    let frame = 0;
    const draw = (now: number) => {
      frame = requestAnimationFrame(draw);
      const seconds = Math.min((now - last) / 1000, 0.1);
      last = now;

      if (tap) tap.getFloatFrequencyData(decibels);
      else decibels.fill(-Infinity);
      ranges.forEach((range, bar) => {
        const level = Math.min(1, Math.max(0, (bandLevel(decibels, range) - FLOOR_DB) / (CEILING_DB - FLOOR_DB)));
        levels[bar] = Math.max(level, levels[bar]! - FALL_PER_SECOND * seconds);
        const peak = peaks[bar]!;
        if (levels[bar]! >= peak.level) {
          peak.level = levels[bar]!;
          peak.heldFor = 0;
        } else if ((peak.heldFor += seconds) > PEAK_HOLD_SECONDS) {
          peak.level = Math.max(levels[bar]!, peak.level - PEAK_FALL_PER_SECOND * seconds);
        }
      });

      const style = styleRef.current;
      const px = window.devicePixelRatio;
      const { width, height } = canvas;

      if (style === 'led') {
        if (led?.width !== width || led.height !== height) led = ledLayout(width, height, px);
        context.drawImage(led.background, 0, 0);
        // Same circle as the unlit lamp. A glow or highlight reads as a second ring.
        context.globalCompositeOperation = 'source-over';
        context.globalAlpha = 1;
        for (let column = 0; column < LED_COLUMNS; column++) {
          const level = Math.max(levels[column * 2]!, levels[column * 2 + 1]!);
          const peak = Math.max(peaks[column * 2]!.level, peaks[column * 2 + 1]!.level);
          const lit = Math.round(level * led.rows);
          const peakRow = Math.min(led.rows, Math.round(peak * led.rows)) - 1;
          for (let row = 0; row < led.rows; row++) {
            if (row >= lit && row !== peakRow) continue;
            context.fillStyle = LED_COLORS[ledColor(row, led.rows)];
            context.beginPath();
            context.arc(
              (column + 0.5) * led.slot,
              led.base - (row + 0.5) * led.rowPitch,
              led.radius,
              0,
              Math.PI * 2,
            );
            context.fill();
          }
        }
        return;
      }

      const palette = MIRRORED_PALETTES[style];
      const top = height * 0.05;
      const base = height * 0.64;
      const span = base - top;
      const slot = width / BAR_COUNT;
      const barWidth = Math.max(px, slot - Math.max(px, slot * 0.22));
      const capHeight = Math.max(2 * px, Math.round(span * 0.008));

      const background = context.createLinearGradient(0, 0, 0, height);
      background.addColorStop(0, palette.backgroundEdge);
      background.addColorStop(0.64, palette.backgroundMiddle);
      background.addColorStop(1, palette.backgroundEdge);
      context.fillStyle = background;
      context.fillRect(0, 0, width, height);

      const bars = context.createLinearGradient(0, base, 0, top);
      palette.bars.forEach((color, index) => bars.addColorStop(index / (palette.bars.length - 1), color));
      context.fillStyle = bars;
      for (let bar = 0; bar < BAR_COUNT; bar++) {
        const barHeight = levels[bar]! * span;
        context.fillRect(bar * slot, base - barHeight, barWidth, barHeight);
      }

      // The reflection: the same bars, squashed and fading, below the baseline.
      const reflection = context.createLinearGradient(0, base, 0, base + span * 0.45);
      reflection.addColorStop(0, palette.reflection[0]);
      reflection.addColorStop(1, palette.reflection[1]);
      context.fillStyle = reflection;
      for (let bar = 0; bar < BAR_COUNT; bar++) {
        context.fillRect(bar * slot, base + px, barWidth, levels[bar]! * span * 0.45);
      }
      context.fillStyle = palette.baseline;
      context.fillRect(0, base, width, px);

      context.fillStyle = palette.peak;
      for (let bar = 0; bar < BAR_COUNT; bar++) {
        const peak = peaks[bar]!.level;
        if (peak <= 0.01) continue;
        context.fillRect(bar * slot, base - peak * span - capHeight * 2, barWidth, capHeight);
      }
    };
    draw(performance.now());

    return () => {
      cancelAnimationFrame(frame);
      observer.disconnect();
      if (analyser && tap) analyser.disconnect(tap);
    };
  }, [analyser]);

  return (
    <>
      <canvas ref={canvasRef} className="visualizer-canvas" />
      {!analyser && <p className="visualizer-message">Press play to start the equalizer.</p>}
      {topSlot &&
        createPortal(
          <div className="milkdrop-presets">
            <button aria-label="Previous style" onClick={() => stepStyle(-1)}>
              ‹
            </button>
            <span className="milkdrop-preset-name">{STYLES[styleIndex]!.label}</span>
            <button aria-label="Next style" onClick={() => stepStyle(1)}>
              ›
            </button>
          </div>,
          topSlot,
        )}
    </>
  );
}
