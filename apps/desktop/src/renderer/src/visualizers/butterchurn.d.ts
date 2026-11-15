declare module 'butterchurn' {
  export type MilkDropPreset = Record<string, unknown>;

  export interface VisualizerOptions {
    width: number;
    height: number;
    pixelRatio?: number;
    textureRatio?: number;
  }

  export interface Visualizer {
    connectAudio(node: AudioNode): void;
    disconnectAudio(node: AudioNode): void;
    loadPreset(preset: MilkDropPreset, blendSeconds?: number): void;
    setRendererSize(width: number, height: number, options?: Partial<VisualizerOptions>): void;
    render(): void;
  }

  const butterchurn: {
    createVisualizer(context: BaseAudioContext, canvas: HTMLCanvasElement, options: VisualizerOptions): Visualizer;
  };
  export default butterchurn;
}

declare module 'butterchurn-presets' {
  import type { MilkDropPreset } from 'butterchurn';

  const presets: { getPresets(): Record<string, MilkDropPreset> };
  export default presets;
}
