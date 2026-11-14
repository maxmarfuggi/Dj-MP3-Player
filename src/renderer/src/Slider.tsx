import type { CSSProperties } from 'react';

interface SliderProps {
  label: string;
  value: number;
  max: number;
  step: number;
  disabled?: boolean;
  onChange(value: number): void;
}

/** A range input whose track is filled up to the current value. */
export function Slider({ label, value, max, step, disabled, onChange }: SliderProps) {
  const filled = max > 0 ? Math.min(Math.max(value / max, 0), 1) : 0;
  return (
    <input
      type="range"
      className="slider"
      aria-label={label}
      min={0}
      max={max}
      step={step}
      value={value}
      disabled={disabled}
      style={{ '--filled': `${filled * 100}%` } as CSSProperties}
      onChange={(event) => onChange(Number(event.target.value))}
    />
  );
}
