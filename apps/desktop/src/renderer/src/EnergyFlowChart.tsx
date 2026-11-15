import { type CSSProperties, type PointerEvent, memo, useCallback, useId, useMemo, useState } from 'react';
import { type HarmonicKind, bpmGrid, energyFlow } from './energyFlow';
import { formatBpm } from './format';
import { camelotHue } from './keyColor';
import type { TrackEntry } from './trackOrder';

interface EnergyFlowChartProps {
  entries: TrackEntry[];
  /** Position of the playing track in `entries`, or -1. */
  currentIndex: number;
  isPlaying: boolean;
  onPlay(index: number): void;
}

const HEIGHT = 140;
const PAD = { top: 24, right: 18, bottom: 16, left: 40 };
/** Dots are dropped when tracks sit closer together than this, leaving just the line. */
const MIN_DOT_SPACING = 7;
/** Keeps a set at one tempo from filling the whole height with tiny wobbles. */
const MIN_BPM_SPAN = 8;
const TOOLTIP_HALF_WIDTH = 125;
const NO_KEY_COLOR = 'hsl(230 8% 58%)';

const KIND_LABELS: Record<HarmonicKind, string> = {
  smooth: 'Smooth',
  shift: 'Shift',
  clash: 'Clash',
};

const keyColor = (number: number, lightness = 62) => `hsl(${camelotHue(number)} 85% ${lightness}%)`;

function useWidth() {
  const [width, setWidth] = useState(0);
  const ref = useCallback((element: HTMLElement | null) => {
    if (!element) return;
    const observer = new ResizeObserver(([entry]) => setWidth(entry!.contentRect.width));
    observer.observe(element);
    return () => observer.disconnect();
  }, []);
  return [ref, width] as const;
}

/** An S-shaped curve between two points that never overshoots either of them. */
function curve(x0: number, y0: number, x1: number, y1: number): string {
  const mid = (x0 + x1) / 2;
  return `C${mid} ${y0} ${mid} ${y1} ${x1} ${y1}`;
}

function formatChange(change: number): string {
  const percent = (change * 100).toFixed(1);
  return `${change > 0 ? '+' : change < 0 ? '−' : '±'}${percent.replace('-', '')}%`;
}

/**
 * The tempo of a list in its current order, coloured by key, with each mix
 * rated by how well the keys go together and flagged where the tempo jumps.
 */
export const EnergyFlowChart = memo(function EnergyFlowChart({
  entries,
  currentIndex,
  isPlaying,
  onPlay,
}: EnergyFlowChartProps) {
  const id = useId();
  const [ref, width] = useWidth();
  const [hover, setHover] = useState<number | null>(null);
  const flow = useMemo(() => energyFlow(entries.map((entry) => entry.track)), [entries]);

  const known = flow.points.flatMap((point, index) =>
    point.level === null ? [] : [{ ...point, level: point.level, index }],
  );
  if (known.length < 2) {
    return <p className="energy-flow energy-flow-empty">Not enough tracks with a BPM to chart.</p>;
  }

  const rated = flow.counts.smooth + flow.counts.shift + flow.counts.clash;
  const harmonic = rated === 0 ? null : Math.round(((flow.counts.smooth + flow.counts.shift) / rated) * 100);

  let low = Math.min(...known.map((point) => point.level));
  let high = Math.max(...known.map((point) => point.level));
  const extra = Math.max(0, MIN_BPM_SPAN - (high - low));
  low -= extra / 2;
  high += extra / 2;
  // A little headroom so the highest and lowest dots are not cut off.
  const margin = (high - low) * 0.08;
  low -= margin;
  high += margin;

  const plotWidth = Math.max(0, width - PAD.left - PAD.right);
  const plotHeight = HEIGHT - PAD.top - PAD.bottom;
  const count = entries.length;
  const spacing = count > 1 ? plotWidth / (count - 1) : 0;
  const x = (index: number) => PAD.left + (count > 1 ? index * spacing : plotWidth / 2);
  const y = (level: number) => PAD.top + ((high - level) / (high - low)) * plotHeight;
  const showDots = spacing >= MIN_DOT_SPACING;
  const pointColor = (point: (typeof known)[number], lightness?: number) =>
    point.key ? keyColor(point.key.number, lightness) : NO_KEY_COLOR;

  const linePath = known
    .map((point, n) => {
      const previous = known[n - 1];
      return previous
        ? curve(x(previous.index), y(previous.level), x(point.index), y(point.level))
        : `M${x(point.index)} ${y(point.level)}`;
    })
    .join('');
  const first = known[0]!;
  const last = known[known.length - 1]!;
  const areaPath = `${linePath}L${x(last.index)} ${HEIGHT - PAD.bottom}L${x(first.index)} ${HEIGHT - PAD.bottom}Z`;

  const segments = known.slice(1).map((point, n) => {
    const previous = known[n]!;
    const adjacent = point.index === previous.index + 1;
    const mix = adjacent ? flow.mixes[previous.index]! : null;
    return {
      key: point.index,
      d: `M${x(previous.index)} ${y(previous.level)}${curve(x(previous.index), y(previous.level), x(point.index), y(point.level))}`,
      midX: (x(previous.index) + x(point.index)) / 2,
      midY: (y(previous.level) + y(point.level)) / 2,
      // Tracks without a BPM in between leave a faint bridge rather than a rated mix.
      kind: adjacent ? (mix?.harmonic?.kind ?? 'unrated') : 'bridge',
      tempoJump: mix?.tempoJump ?? false,
    };
  });

  const onPointerMove = (event: PointerEvent<SVGSVGElement>) => {
    const left = event.currentTarget.getBoundingClientRect().left;
    const index = count > 1 ? Math.round((event.clientX - left - PAD.left) / spacing) : 0;
    setHover(Math.max(0, Math.min(count - 1, index)));
  };

  const current = currentIndex >= 0 ? known.find((point) => point.index === currentIndex) : undefined;
  const hoverEntry = hover === null ? undefined : entries[hover];
  const hoverPoint = hover === null ? undefined : flow.points[hover];
  const mixIn = hover === null || hover === 0 ? undefined : flow.mixes[hover - 1];

  return (
    <section className={`energy-flow${isPlaying ? '' : ' paused'}`}>
      <div className="energy-flow-header">
        <h2 className="energy-flow-title">Energy flow</h2>
        <div className="energy-flow-stats">
          {(Object.keys(KIND_LABELS) as HarmonicKind[]).map((kind) => (
            <span key={kind} className={`energy-flow-stat energy-flow-kind-${kind}`}>
              <i />
              {KIND_LABELS[kind]}
              <b>{flow.counts[kind]}</b>
            </span>
          ))}
          <span
            className="energy-flow-stat energy-flow-kind-tempo"
            title="Mixes with more than a 6% tempo change, allowing for half and double time"
          >
            <i />
            Tempo jumps
            <b>{flow.tempoJumps}</b>
          </span>
          {harmonic !== null && (
            <span className="energy-flow-score" title="Key changes that are smooth or a deliberate shift">
              <b>{harmonic}%</b> harmonic
            </span>
          )}
        </div>
      </div>

      <div className="energy-flow-chart" ref={ref}>
        {width > 0 && (
          <svg
            width={width}
            height={HEIGHT}
            role="img"
            aria-label={`Energy flow: ${flow.counts.smooth} smooth, ${flow.counts.shift} shifting and ${flow.counts.clash} clashing key changes, ${flow.tempoJumps} tempo jumps`}
            onPointerMove={onPointerMove}
            onPointerLeave={() => setHover(null)}
            onClick={() => hover !== null && onPlay(hover)}
          >
            <defs>
              <linearGradient
                id={`${id}-keys`}
                gradientUnits="userSpaceOnUse"
                x1={PAD.left}
                x2={PAD.left + plotWidth}
                y1={0}
                y2={0}
              >
                {known.map((point) => (
                  <stop
                    key={point.index}
                    offset={plotWidth > 0 ? (x(point.index) - PAD.left) / plotWidth : 0}
                    stopColor={pointColor(point)}
                  />
                ))}
              </linearGradient>
              <linearGradient id={`${id}-fade`} x1={0} x2={0} y1={0} y2={1}>
                <stop offset={0} stopColor="#fff" stopOpacity={0.32} />
                <stop offset={1} stopColor="#fff" stopOpacity={0} />
              </linearGradient>
              <mask id={`${id}-area`}>
                <rect x={0} y={0} width={width} height={HEIGHT} fill={`url(#${id}-fade)`} />
              </mask>
              <filter id={`${id}-glow`} x="-10%" y="-50%" width="120%" height="200%">
                <feGaussianBlur stdDeviation={5} />
              </filter>
            </defs>

            {bpmGrid(low, high).map((bpm) => (
              <g key={bpm} className="energy-flow-grid">
                <line x1={PAD.left} x2={PAD.left + plotWidth} y1={y(bpm)} y2={y(bpm)} />
                <text x={PAD.left - 10} y={y(bpm)}>
                  {bpm}
                </text>
              </g>
            ))}

            <path d={areaPath} fill={`url(#${id}-keys)`} mask={`url(#${id}-area)`} />
            <path className="energy-flow-glow" d={linePath} stroke={`url(#${id}-keys)`} filter={`url(#${id}-glow)`} />
            {segments.map((segment) => (
              <path
                key={segment.key}
                className={`energy-flow-line energy-flow-line-${segment.kind}`}
                d={segment.d}
                stroke={segment.kind === 'bridge' ? undefined : `url(#${id}-keys)`}
              />
            ))}
            {segments.map(
              (segment) =>
                segment.kind === 'clash' && (
                  <circle key={segment.key} className="energy-flow-clash" cx={segment.midX} cy={segment.midY} r={2.5} />
                ),
            )}
            {segments.map(
              (segment) =>
                segment.tempoJump && (
                  <rect
                    key={segment.key}
                    className="energy-flow-tempo-jump"
                    x={segment.midX - 3.5}
                    y={segment.midY - 3.5}
                    width={7}
                    height={7}
                    transform={`rotate(45 ${segment.midX} ${segment.midY})`}
                  />
                ),
            )}

            {hover !== null && (
              <line
                className="energy-flow-cursor"
                x1={x(hover)}
                x2={x(hover)}
                y1={PAD.top - 8}
                y2={HEIGHT - PAD.bottom}
              />
            )}

            {current && (
              <g className="energy-flow-current" style={{ '--point-color': pointColor(current) } as CSSProperties}>
                <line x1={x(current.index)} x2={x(current.index)} y1={PAD.top - 8} y2={HEIGHT - PAD.bottom} />
                <circle className="energy-flow-pulse" cx={x(current.index)} cy={y(current.level)} r={6} />
              </g>
            )}

            {known.map((point) => {
              const active = point.index === currentIndex || point.index === hover;
              if (!showDots && !active) return null;
              const color = pointColor(point);
              const major = point.key?.letter === 'B';
              return (
                <circle
                  key={point.index}
                  className="energy-flow-dot"
                  cx={x(point.index)}
                  cy={y(point.level)}
                  r={active ? 5.5 : 3.5}
                  fill={major ? 'var(--bg)' : color}
                  stroke={color}
                  strokeWidth={major ? 2 : 0}
                />
              );
            })}

            {current && (
              <text
                className="energy-flow-current-label"
                x={x(current.index)}
                y={PAD.top - 13}
                style={{ fill: pointColor(current, 75) }}
              >
                {formatBpm(current.bpm!)}
                {current.key && ` · ${current.key.number}${current.key.letter}`}
              </text>
            )}
          </svg>
        )}

        {hover !== null && hoverEntry && hoverPoint && (
          <div
            className="energy-flow-tooltip"
            style={{ left: Math.max(TOOLTIP_HALF_WIDTH, Math.min(width - TOOLTIP_HALF_WIDTH, x(hover))) }}
          >
            <div className="energy-flow-tooltip-top">
              <span className="energy-flow-tooltip-index">{hover + 1}</span>
              <span className="energy-flow-tooltip-title">{hoverEntry.track.title}</span>
              {hoverPoint.key && (
                <span
                  className="energy-flow-tooltip-key"
                  style={{ '--key-hue': camelotHue(hoverPoint.key.number) } as CSSProperties}
                >
                  {hoverPoint.key.number}
                  {hoverPoint.key.letter}
                </span>
              )}
            </div>
            <div className="energy-flow-tooltip-artist">
              {hoverPoint.bpm !== null && <span className="energy-flow-tooltip-bpm">{formatBpm(hoverPoint.bpm)} BPM</span>}
              {hoverEntry.track.artist}
            </div>
            {mixIn && (mixIn.harmonic || mixIn.tempoChange !== null) && (
              <div className="energy-flow-tooltip-mix">
                {mixIn.harmonic && (
                  <span className={`energy-flow-kind-${mixIn.harmonic.kind}`}>
                    <i />
                    {mixIn.harmonic.label}
                  </span>
                )}
                {mixIn.tempoChange !== null && (
                  <span className={mixIn.tempoJump ? 'energy-flow-kind-tempo' : undefined}>
                    {mixIn.tempoJump && <i />}
                    {formatChange(mixIn.tempoChange)} tempo{mixIn.halfTime && ' (half/double time)'}
                  </span>
                )}
              </div>
            )}
          </div>
        )}
      </div>
    </section>
  );
});
