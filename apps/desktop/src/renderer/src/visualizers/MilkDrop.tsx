import { useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import type { MilkDropPreset, Visualizer } from 'butterchurn';

interface MilkDropProps {
  analyser: AnalyserNode | null;
  /** Where the preset picker goes, in the visualizer's top bar. */
  topSlot: HTMLElement | null;
}

const PRESET_SECONDS = 25;
const BLEND_SECONDS = 2.7;

interface PresetList {
  names: string[];
  presets: Record<string, MilkDropPreset>;
}

function shuffle<T>(items: T[]): T[] {
  const result = [...items];
  for (let i = result.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    [result[i], result[j]] = [result[j]!, result[i]!];
  }
  return result;
}

export function MilkDrop({ analyser, topSlot }: MilkDropProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const visualizerRef = useRef<Visualizer | null>(null);
  const [presetList, setPresetList] = useState<PresetList | null>(null);
  const [presetIndex, setPresetIndex] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const firstPreset = useRef(true);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!analyser || !canvas) return;
    let cancelled = false;
    let frame = 0;
    let observer: ResizeObserver | undefined;
    let visualizer: Visualizer | undefined;

    // Loaded on demand: butterchurn and its presets are close to 1 MB.
    Promise.all([import('butterchurn'), import('butterchurn-presets')])
      .then(([{ default: butterchurn }, { default: presetPack }]) => {
        if (cancelled) return;
        // Butterchurn draws its final image at width × height without applying
        // pixelRatio, so it renders at CSS pixels and the canvas is scaled up.
        const resize = () => {
          canvas.width = canvas.clientWidth;
          canvas.height = canvas.clientHeight;
          visualizer?.setRendererSize(canvas.width, canvas.height, { pixelRatio: 1 });
        };
        resize();
        try {
          visualizer = butterchurn.createVisualizer(analyser.context, canvas, {
            width: canvas.width,
            height: canvas.height,
            pixelRatio: 1,
          });
        } catch {
          setError('MilkDrop needs WebGL 2, which is not available here.');
          return;
        }
        visualizer.connectAudio(analyser);
        visualizerRef.current = visualizer;
        observer = new ResizeObserver(resize);
        observer.observe(canvas);

        const presets = presetPack.getPresets();
        firstPreset.current = true;
        setPresetList({ names: shuffle(Object.keys(presets)), presets });
        setPresetIndex(0);

        const draw = () => {
          frame = requestAnimationFrame(draw);
          visualizer?.render();
        };
        draw();
      })
      .catch(() => setError('MilkDrop could not be loaded.'));

    return () => {
      cancelled = true;
      cancelAnimationFrame(frame);
      observer?.disconnect();
      visualizer?.disconnectAudio(analyser);
      visualizerRef.current = null;
    };
  }, [analyser]);

  useEffect(() => {
    const visualizer = visualizerRef.current;
    const name = presetList?.names[presetIndex];
    if (!visualizer || !presetList || name === undefined) return;
    visualizer.loadPreset(presetList.presets[name]!, firstPreset.current ? 0 : BLEND_SECONDS);
    firstPreset.current = false;
    const timer = window.setTimeout(() => stepPreset(1), PRESET_SECONDS * 1000);
    return () => window.clearTimeout(timer);
  }, [presetList, presetIndex]);

  const stepPreset = (step: number) => {
    setPresetIndex((index) => {
      const count = presetList?.names.length ?? 1;
      return (index + step + count) % count;
    });
  };

  const presetName = presetList?.names[presetIndex];

  return (
    <>
      <canvas ref={canvasRef} className="visualizer-canvas milkdrop-canvas" />
      {!analyser && <p className="visualizer-message">Press play to start MilkDrop.</p>}
      {error && <p className="visualizer-message">{error}</p>}
      {presetName !== undefined &&
        topSlot &&
        createPortal(
          <div className="milkdrop-presets">
            <button aria-label="Previous preset" onClick={() => stepPreset(-1)}>
              ‹
            </button>
            <span className="milkdrop-preset-name" title={presetName}>
              {presetName}
            </span>
            <button aria-label="Next preset" onClick={() => stepPreset(1)}>
              ›
            </button>
          </div>,
          topSlot,
        )}
    </>
  );
}
