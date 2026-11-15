import { useEffect, useRef } from 'react';

const BACKGROUND = '#0b0c10';
/** How much of the previous frame stays visible, for a soft trail. */
const TRAIL_FADE = 'rgba(11, 12, 16, 0.35)';
/** The colours go once around the colour wheel in this time. */
const COLOR_CYCLE_SECONDS = 20;
/** Hue offsets of the gradient's three stops, from left to right. */
const GRADIENT_HUES = [0, 55, 120];

const color = (hue: number) => `hsl(${hue % 360} 100% 68%)`;

/** Draws the audio being played as a live oscilloscope line. */
export function Waveform({ analyser }: { analyser: AnalyserNode | null }) {
  const canvasRef = useRef<HTMLCanvasElement>(null);

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

      const hue = (now / 1000 / COLOR_CYCLE_SECONDS) * 360;
      const gradient = context.createLinearGradient(0, 0, width, 0);
      GRADIENT_HUES.forEach((offset, index) => {
        gradient.addColorStop(index / (GRADIENT_HUES.length - 1), color(hue + offset));
      });

      const middle = height / 2;
      const amplitude = height * 0.42;
      context.beginPath();
      for (let i = 0; i < visible; i++) {
        const x = (i / (visible - 1)) * width;
        const y = middle - samples[start + i]! * amplitude;
        if (i === 0) context.moveTo(x, y);
        else context.lineTo(x, y);
      }
      context.lineJoin = 'round';
      context.strokeStyle = gradient;
      context.lineWidth = 3 * window.devicePixelRatio;
      context.shadowColor = color(hue + GRADIENT_HUES[1]!);
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

  return <canvas ref={canvasRef} className="visualizer-canvas" />;
}
