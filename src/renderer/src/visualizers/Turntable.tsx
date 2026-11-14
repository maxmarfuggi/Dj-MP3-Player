import { type CSSProperties, memo, useEffect, useMemo, useRef } from 'react';
import type { Track } from '../../../shared/types';
import { labelColor } from '../colors';
import { layoutLabelText, type LabelText, type MeasureText } from './labelLayout';
import { armAngleForRadius } from './tonearmGeometry';

interface TurntableProps {
  track: Track | null;
  isPlaying: boolean;
  currentTime: number;
  duration: number;
}

/** 33⅓ RPM in degrees per second. */
const RECORD_SPEED = (100 / 3 / 60) * 360;
/** How quickly the platter reaches speed or stops, per second. */
const PLATTER_RESPONSE = 2.5;

// Everything is drawn on a 1000 × 760 deck, lit from the top left.
const DECK = '0 0 1000 760';
const CENTER = { x: 400, y: 380 };
const PLATTER_RADIUS = 335;
const RECORD_RADIUS = 300;
const LABEL_RADIUS = 100;
/** The music is cut between these radii; the stylus travels from the first to the second. */
const FIRST_GROOVE = 291;
const LAST_GROOVE = 121;
/** Radii of the silent gaps between songs, as on an LP with several of them. */
const TRACK_GAPS = [149, 185, 220, 261];
const GROOVE_PITCH = 2.1;

const PIVOT = { x: 836, y: 176 };
// The arm is drawn pointing straight down from its pivot and then rotated.
// The headshell sits at the end of the tube, angled in towards the spindle.
const HEADSHELL = { x: -24, y: 442, angle: 18 };
/** How far along the headshell the stylus is. */
const STYLUS_DISTANCE = 66;
const HEADSHELL_RADIANS = (HEADSHELL.angle * Math.PI) / 180;
const ARM = {
  pivot: PIVOT,
  center: CENTER,
  stylus: {
    x: HEADSHELL.x - STYLUS_DISTANCE * Math.sin(HEADSHELL_RADIANS),
    y: HEADSHELL.y + STYLUS_DISTANCE * Math.cos(HEADSHELL_RADIANS),
  },
};
/** Parked on the arm rest, clear of the platter. */
const ARM_REST_ANGLE = -5;
const ARM_TUBE = 'M0 -4 V280 C0 335 -24 350 -24 405 V444';
/** The rest holds the parked arm part of the way along its straight section. */
const ARM_REST_DISTANCE = 250;
const ARM_REST = {
  x: PIVOT.x - ARM_REST_DISTANCE * Math.sin((ARM_REST_ANGLE * Math.PI) / 180),
  y: PIVOT.y + ARM_REST_DISTANCE * Math.cos((ARM_REST_ANGLE * Math.PI) / 180),
};

const at = { cx: CENTER.x, cy: CENTER.y };

/** Where the record sits on the deck, for layers that cover only the record. */
const RECORD_BOX: CSSProperties = {
  left: `${(CENTER.x - RECORD_RADIUS) / 10}%`,
  top: `${(CENTER.y - RECORD_RADIUS) / 7.6}%`,
  width: `${(RECORD_RADIUS * 2) / 10}%`,
  height: `${(RECORD_RADIUS * 2) / 7.6}%`,
};
const RECORD_VIEW = `${CENTER.x - RECORD_RADIUS} ${CENTER.y - RECORD_RADIUS} ${RECORD_RADIUS * 2} ${RECORD_RADIUS * 2}`;

/** A repeatable stand-in for randomness, so the record looks the same on every render. */
const noise = (seed: number) => Math.abs(Math.sin(seed * 12.9898) * 43758.5453) % 1;

/** Fine rings of uneven brightness, as louder and quieter passages are. */
const GROOVES = (() => {
  const rings: { radius: number; opacity: number }[] = [];
  for (let radius = FIRST_GROOVE, index = 0; radius >= LAST_GROOVE; radius -= GROOVE_PITCH, index++) {
    if (TRACK_GAPS.some((gap) => Math.abs(radius - gap) < 2.4)) continue;
    rings.push({ radius, opacity: 0.035 + noise(index) * 0.075 });
  }
  return rings;
})();

/** The light streaks only show on the grooves: fainter on the smooth vinyl, gone on the label. */
const SHEEN_MASK = (() => {
  const stop = (alpha: number, radius: number) =>
    `rgba(0, 0, 0, ${alpha}) ${((radius / RECORD_RADIUS) * 100).toFixed(2)}%`;
  const stops = [
    stop(0, LABEL_RADIUS + 1),
    stop(0.45, LABEL_RADIUS + 3),
    stop(0.45, LAST_GROOVE - 4),
    stop(1, LAST_GROOVE),
    ...TRACK_GAPS.flatMap((gap) => [stop(1, gap - 3), stop(0.4, gap - 1.5), stop(0.4, gap + 1.5), stop(1, gap + 3)]),
    stop(1, FIRST_GROOVE),
    stop(0.5, FIRST_GROOVE + 2.5),
    stop(0.5, RECORD_RADIUS - 1.5),
    stop(0, RECORD_RADIUS),
  ];
  return `radial-gradient(closest-side, ${stops.join(', ')})`;
})();

/** Rows of dots around the platter's edge, which a real deck's strobe lamp lights. */
const STROBE_ROWS = [
  { radius: 328.5, dots: 180 },
  { radius: 323, dots: 174 },
  { radius: 317.5, dots: 168 },
  { radius: 312, dots: 162 },
].map(({ radius, dots }) => {
  const spacing = (2 * Math.PI * radius) / dots;
  return { radius, dash: `${(spacing * 0.42).toFixed(3)} ${(spacing * 0.58).toFixed(3)}` };
});

/** Faint scuffs and dust that turn with the record, so the spin shows outside the label too. */
const SCUFFS = [
  { radius: 276, start: 20, sweep: 34 },
  { radius: 239, start: 142, sweep: 21 },
  { radius: 203, start: 251, sweep: 38 },
  { radius: 166, start: 78, sweep: 17 },
  { radius: 284, start: 200, sweep: 46 },
  { radius: 131, start: 305, sweep: 28 },
].map(({ radius, start, sweep }) => {
  const point = (degrees: number) => {
    const radians = (degrees * Math.PI) / 180;
    return `${(CENTER.x + radius * Math.cos(radians)).toFixed(1)} ${(CENTER.y + radius * Math.sin(radians)).toFixed(1)}`;
  };
  return `M${point(start)} A${radius} ${radius} 0 0 1 ${point(start + sweep)}`;
});
const DUST = Array.from({ length: 14 }, (_, index) => {
  const radius = LAST_GROOVE + noise(index + 40) * (FIRST_GROOVE - LAST_GROOVE);
  const radians = noise(index + 80) * 2 * Math.PI;
  return { cx: CENTER.x + radius * Math.cos(radians), cy: CENTER.y + radius * Math.sin(radians) };
});

const HEADSHELL_WIRES = [
  { color: '#e5484d', x: -6 },
  { color: '#f2f3f7', x: -2 },
  { color: '#3fb86b', x: 2 },
  { color: '#4f8dff', x: 6 },
];

const LABEL_FONT = 'system-ui, -apple-system, sans-serif';
// Inside the ring drawn just within the label's edge.
const LABEL_TEXT_RADIUS = 86;

let measureContext: CanvasRenderingContext2D | null = null;

function textMeasurer(weight: number): MeasureText {
  measureContext ??= document.createElement('canvas').getContext('2d');
  return (text, fontSize) => {
    if (!measureContext) return text.length * fontSize * 0.6;
    measureContext.font = `${weight} ${fontSize}px ${LABEL_FONT}`;
    return measureContext.measureText(text).width;
  };
}

export function Turntable({ track, isPlaying, currentTime, duration }: TurntableProps) {
  const recordRef = useRef<SVGSVGElement>(null);
  // The spin runs as a browser animation, off the main thread. Script only steps in
  // to ease its speed while the platter starts or stops.
  const spinRef = useRef<Animation | null>(null);
  useEffect(() => {
    const spin = recordRef.current?.animate([{ transform: 'rotate(0deg)' }, { transform: 'rotate(360deg)' }], {
      duration: (360 / RECORD_SPEED) * 1000,
      iterations: Infinity,
    });
    if (!spin) return;
    spin.playbackRate = 0;
    spinRef.current = spin;
    return () => spin.cancel();
  }, []);

  useEffect(() => {
    const spin = spinRef.current;
    if (!spin) return;
    const target = isPlaying ? 1 : 0;
    let speed = spin.playbackRate;
    let last = performance.now();
    let frame = 0;
    const ease = (now: number) => {
      const seconds = Math.min((now - last) / 1000, 0.1);
      last = now;
      speed += (target - speed) * Math.min(1, seconds * PLATTER_RESPONSE);
      // Easing alone never quite reaches the target.
      if (Math.abs(target - speed) < 0.003) speed = target;
      spin.playbackRate = speed;
      if (speed !== target) frame = requestAnimationFrame(ease);
    };
    frame = requestAnimationFrame(ease);
    return () => cancelAnimationFrame(frame);
  }, [isPlaying]);

  const progress = duration > 0 ? Math.min(currentTime / duration, 1) : 0;
  // Rounded so the arm only re-renders when it has moved visibly, not on every time update.
  const armAngle = track
    ? Math.round(armAngleForRadius(ARM, FIRST_GROOVE + (LAST_GROOVE - FIRST_GROOVE) * progress) * 10) / 10
    : ARM_REST_ANGLE;

  const title = track?.title ?? 'No track';
  const artist = track?.artist ?? '';
  const titleText = useMemo(
    () =>
      layoutLabelText(title, {
        measure: textMeasurer(700),
        radius: LABEL_TEXT_RADIUS,
        fontSizes: [15, 13, 11.5],
        maxLines: 3,
        anchorY: -24,
        direction: 'up',
      }),
    [title],
  );
  const artistText = useMemo(
    () =>
      layoutLabelText(artist, {
        measure: textMeasurer(400),
        radius: LABEL_TEXT_RADIUS,
        fontSizes: [13, 11],
        maxLines: 2,
        anchorY: 36,
        direction: 'down',
      }),
    [artist],
  );

  return (
    <div className="turntable" role="img" aria-label={track ? `${track.title} on a turntable` : 'Turntable'}>
      <div
        className="turntable-stage"
        style={{ '--pivot': `${PIVOT.x / 10}% ${PIVOT.y / 7.6}%` } as CSSProperties}
      >
        <Deck spinning={isPlaying} />

        {/* Only this layer turns. Rings look the same at any angle, so the grooves stay on the deck. */}
        <svg ref={recordRef} className="turntable-record" style={RECORD_BOX} viewBox={RECORD_VIEW} aria-hidden="true">
          <circle {...at} r={FIRST_GROOVE} fill="url(#tt-warp)" />
          {SCUFFS.map((path) => (
            <path key={path} d={path} fill="none" stroke="#fff" strokeOpacity="0.07" strokeWidth="0.9" />
          ))}
          {DUST.map((speck) => (
            <circle key={`${speck.cx}/${speck.cy}`} {...speck} r="0.9" fill="#fff" fillOpacity="0.28" />
          ))}

          <circle {...at} r={LABEL_RADIUS} fill={labelColor(track)} />
          <circle {...at} r={LABEL_RADIUS} fill="url(#tt-label-shade)" />
          <circle {...at} r={LABEL_RADIUS - 6} fill="none" stroke="#000" strokeOpacity="0.2" strokeWidth="1" />
          {/* The ring pressed into the paper around the spindle hole. */}
          <circle {...at} r="33" fill="none" stroke="#000" strokeOpacity="0.16" strokeWidth="1.4" />
          <circle {...at} r="31.6" fill="none" stroke="#fff" strokeOpacity="0.2" strokeWidth="0.8" />
          <LabelLines text={titleText} className="turntable-label-title" />
          <LabelLines text={artistText} className="turntable-label-artist" />
          {track && (
            <>
              <text x={CENTER.x - 60} y={CENTER.y + 2.5} className="turntable-label-small">
                SIDE A
              </text>
              <text x={CENTER.x + 60} y={CENTER.y + 2.5} className="turntable-label-small">
                STEREO
              </text>
            </>
          )}
          <text x={CENTER.x} y={CENTER.y + 72} className="turntable-label-rpm">
            33⅓ RPM
          </text>
          <circle {...at} r="8.5" fill="#040405" />
        </svg>

        {/* The light on the record stays put while the record turns underneath it. */}
        <div className="turntable-sheen" style={{ ...RECORD_BOX, maskImage: SHEEN_MASK }} />
        <svg className="turntable-layer" viewBox={DECK} aria-hidden="true">
          {/* Dark lines between the grooves break the light up into fine streaks. */}
          {GROOVES.map(({ radius, opacity }) => (
            <circle
              key={radius}
              {...at}
              r={radius + GROOVE_PITCH / 2}
              fill="none"
              stroke="#000"
              strokeOpacity={opacity * 3}
              strokeWidth="1"
            />
          ))}
          <circle {...at} r="7.5" fill="url(#tt-dome)" />
        </svg>

        <Tonearm angle={armAngle} shadow />
        <Tonearm angle={armAngle} />
      </div>
    </div>
  );
}

function LabelLines({ text, className }: { text: LabelText; className: string }) {
  return (
    <text className={className} style={{ fontSize: text.fontSize }}>
      {text.lines.map((line, index) => (
        <tspan key={index} x={CENTER.x} y={CENTER.y + line.y}>
          {line.text}
        </tspan>
      ))}
    </text>
  );
}

/** A dark control with a lit top edge, as if raised from the deck. */
function RaisedRect({ x, y, width, height, radius }: { x: number; y: number; width: number; height: number; radius: number }) {
  return (
    <>
      <rect x={x + 2} y={y + 4} width={width} height={height} rx={radius} fill="#000" opacity="0.45" filter="url(#tt-blur-3)" />
      <rect x={x} y={y} width={width} height={height} rx={radius} fill="url(#tt-button)" />
      <rect
        x={x + 0.6}
        y={y + 0.6}
        width={width - 1.2}
        height={height - 1.2}
        rx={radius - 0.6}
        fill="none"
        stroke="url(#tt-bevel)"
        strokeWidth="1.2"
      />
    </>
  );
}

/**
 * Everything that does not move: the plinth, platter, the record's grooves
 * and the controls. Kept apart so it is drawn once, not on every frame.
 */
const Deck = memo(function Deck({ spinning }: { spinning: boolean }) {
  return (
    <svg className="turntable-layer" viewBox={DECK} aria-hidden="true">
      <defs>
        <filter id="tt-blur-3" x="-20%" y="-20%" width="140%" height="140%">
          <feGaussianBlur stdDeviation="3" />
        </filter>
        <filter id="tt-blur-6" x="-20%" y="-20%" width="140%" height="140%">
          <feGaussianBlur stdDeviation="6" />
        </filter>
        <filter id="tt-blur-16" x="-20%" y="-20%" width="140%" height="140%">
          <feGaussianBlur stdDeviation="16" />
        </filter>
        {/* A fine speckle, tiled, for the plinth's matte finish. */}
        <filter id="tt-speckle" x="0" y="0" width="100%" height="100%">
          <feTurbulence type="fractalNoise" baseFrequency="0.9" numOctaves="2" seed="7" stitchTiles="stitch" />
          <feColorMatrix values="0 0 0 0 1  0 0 0 0 1  0 0 0 0 1  0 0 0 0.42 -0.15" />
        </filter>
        <pattern id="tt-grain" width="200" height="200" patternUnits="userSpaceOnUse">
          <rect width="200" height="200" filter="url(#tt-speckle)" />
        </pattern>

        <linearGradient id="tt-plinth" x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stopColor="#353840" />
          <stop offset="0.55" stopColor="#25272e" />
          <stop offset="1" stopColor="#191a20" />
        </linearGradient>
        <radialGradient id="tt-plinth-light" cx="0.22" cy="0.1" r="0.85">
          <stop offset="0" stopColor="#fff" stopOpacity="0.1" />
          <stop offset="1" stopColor="#fff" stopOpacity="0" />
        </radialGradient>
        {/* An edge that is lit along the top and in shadow along the bottom. */}
        <linearGradient id="tt-bevel" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="#fff" stopOpacity="0.3" />
          <stop offset="0.15" stopColor="#fff" stopOpacity="0.06" />
          <stop offset="0.85" stopColor="#000" stopOpacity="0.2" />
          <stop offset="1" stopColor="#000" stopOpacity="0.55" />
        </linearGradient>
        <linearGradient id="tt-ring-light" x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stopColor="#fff" stopOpacity="0.6" />
          <stop offset="0.5" stopColor="#fff" stopOpacity="0.08" />
          <stop offset="1" stopColor="#fff" stopOpacity="0.3" />
        </linearGradient>
        <linearGradient id="tt-aluminium" x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stopColor="#f3f4f6" />
          <stop offset="0.3" stopColor="#a3a7b0" />
          <stop offset="0.5" stopColor="#dcdee3" />
          <stop offset="0.72" stopColor="#7a7e88" />
          <stop offset="1" stopColor="#b6b9c1" />
        </linearGradient>
        <radialGradient id="tt-dome" cx="0.35" cy="0.3" r="0.75">
          <stop offset="0" stopColor="#fff" />
          <stop offset="0.45" stopColor="#b9bdc6" />
          <stop offset="1" stopColor="#585c66" />
        </radialGradient>
        <radialGradient id="tt-base" cx="0.35" cy="0.3" r="0.9">
          <stop offset="0" stopColor="#3b3e47" />
          <stop offset="1" stopColor="#16171c" />
        </radialGradient>
        <linearGradient id="tt-button" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="#3b3e47" />
          <stop offset="1" stopColor="#212329" />
        </linearGradient>
        <radialGradient id="tt-vinyl">
          <stop offset="0.3" stopColor="#19191c" />
          <stop offset="0.7" stopColor="#101012" />
          <stop offset="1" stopColor="#08080a" />
        </radialGradient>
        <linearGradient id="tt-warp" x1="0" y1="0" x2="1" y2="0.3">
          <stop offset="0" stopColor="#fff" stopOpacity="0.06" />
          <stop offset="0.5" stopColor="#fff" stopOpacity="0" />
          <stop offset="1" stopColor="#fff" stopOpacity="0.035" />
        </linearGradient>
        <radialGradient id="tt-label-shade" cx="0.38" cy="0.32" r="0.8">
          <stop offset="0" stopColor="#fff" stopOpacity="0.22" />
          <stop offset="0.6" stopColor="#fff" stopOpacity="0" />
          <stop offset="1" stopColor="#000" stopOpacity="0.2" />
        </radialGradient>
        {/* Across the counterweight, which is a cylinder lying on its side. */}
        <linearGradient id="tt-weight" gradientUnits="userSpaceOnUse" x1="-25" y1="0" x2="25" y2="0">
          <stop offset="0" stopColor="#4a4e57" />
          <stop offset="0.3" stopColor="#d8dbe0" />
          <stop offset="0.55" stopColor="#999da7" />
          <stop offset="1" stopColor="#35383f" />
        </linearGradient>
        <radialGradient id="tt-red-glow">
          <stop offset="0" stopColor="#ff4a3d" stopOpacity="0.7" />
          <stop offset="1" stopColor="#ff4a3d" stopOpacity="0" />
        </radialGradient>
        <radialGradient id="tt-green-glow">
          <stop offset="0" stopColor="#4dff8f" stopOpacity="0.6" />
          <stop offset="1" stopColor="#4dff8f" stopOpacity="0" />
        </radialGradient>
      </defs>

      {/* Plinth */}
      <rect x="18" y="30" width="964" height="722" rx="30" fill="#000" opacity="0.65" filter="url(#tt-blur-16)" />
      <rect x="10" y="10" width="980" height="740" rx="26" fill="url(#tt-plinth)" />
      <rect x="10" y="10" width="980" height="740" rx="26" fill="url(#tt-grain)" />
      <rect x="10" y="10" width="980" height="740" rx="26" fill="url(#tt-plinth-light)" />
      <rect x="10.75" y="10.75" width="978.5" height="738.5" rx="25.25" fill="none" stroke="url(#tt-bevel)" strokeWidth="1.5" />
      <text x="640" y="722" className="turntable-print" style={{ letterSpacing: '0.32em', opacity: 0.7 }}>
        DIRECT DRIVE
      </text>

      {/* The adapter for 45s, in its recess */}
      <circle cx="66" cy="70" r="31" fill="#0b0c0f" />
      <circle cx="66" cy="70" r="30.4" fill="none" stroke="url(#tt-bevel)" strokeWidth="1.4" transform="rotate(180 66 70)" />
      <circle cx="68" cy="73" r="25" fill="#000" opacity="0.6" filter="url(#tt-blur-3)" />
      <circle cx="66" cy="70" r="24" fill="url(#tt-aluminium)" />
      <circle cx="66" cy="70" r="23.4" fill="none" stroke="url(#tt-ring-light)" strokeWidth="1.2" />
      <circle cx="66" cy="70" r="15" fill="none" stroke="#000" strokeOpacity="0.2" strokeWidth="1" />
      <circle cx="66" cy="70" r="6" fill="#0b0c0f" />

      {/* Platter, with the strobe dots around its edge */}
      <circle cx={CENTER.x + 6} cy={CENTER.y + 10} r={PLATTER_RADIUS + 4} fill="#000" opacity="0.6" filter="url(#tt-blur-6)" />
      <circle {...at} r={PLATTER_RADIUS} fill="url(#tt-aluminium)" />
      <circle {...at} r="320.5" fill="none" stroke="#14151a" strokeWidth="23" />
      {STROBE_ROWS.map(({ radius, dash }) => (
        <circle key={radius} {...at} r={radius} fill="none" stroke="#b4b8c1" strokeWidth="2.7" strokeDasharray={dash} />
      ))}
      <circle {...at} r={PLATTER_RADIUS - 0.6} fill="none" stroke="url(#tt-ring-light)" strokeWidth="1.2" />
      <circle {...at} r="307" fill="#0d0e11" />

      {/* The strobe lamp, which glows on the dots while the platter turns */}
      <circle cx="87" cy="587" r="31" fill="#000" opacity="0.5" filter="url(#tt-blur-3)" />
      <circle cx="84" cy="582" r="30" fill="url(#tt-base)" />
      <circle cx="84" cy="582" r="29.4" fill="none" stroke="url(#tt-ring-light)" strokeWidth="1.2" />
      <circle cx="84" cy="582" r="19" fill="url(#tt-button)" />
      <circle cx="84" cy="582" r="18.4" fill="none" stroke="url(#tt-bevel)" strokeWidth="1.2" />
      <path d="M84 567v8" stroke="#d5d8de" strokeWidth="2" strokeLinecap="round" />
      <g className="turntable-lamp" opacity={spinning ? 1 : 0}>
        <circle cx="120" cy="559" r="64" fill="url(#tt-red-glow)" />
      </g>
      <circle cx="105" cy="568.5" r="4.5" fill={spinning ? '#ff5a4d' : '#5a2320'} />

      {/* Record */}
      <circle cx={CENTER.x + 3} cy={CENTER.y + 5} r={RECORD_RADIUS + 2} fill="#000" opacity="0.75" filter="url(#tt-blur-3)" />
      <circle {...at} r={RECORD_RADIUS} fill="url(#tt-vinyl)" />
      {GROOVES.map(({ radius, opacity }) => (
        <circle key={radius} {...at} r={radius} fill="none" stroke="#fff" strokeOpacity={opacity} strokeWidth="0.9" />
      ))}
      {TRACK_GAPS.map((radius) => (
        <circle key={radius} {...at} r={radius} fill="none" stroke="#fff" strokeOpacity="0.06" strokeWidth="0.7" />
      ))}
      <circle {...at} r={LAST_GROOVE - 4} fill="none" stroke="#fff" strokeOpacity="0.09" strokeWidth="0.8" />
      <circle {...at} r={RECORD_RADIUS - 0.7} fill="none" stroke="url(#tt-ring-light)" strokeWidth="1.3" opacity="0.5" />

      {/* Tonearm base, with the ring that sets the arm's height */}
      <circle cx={PIVOT.x + 4} cy={PIVOT.y + 8} r="68" fill="#000" opacity="0.5" filter="url(#tt-blur-6)" />
      <circle cx={PIVOT.x} cy={PIVOT.y} r="66" fill="url(#tt-base)" />
      <circle cx={PIVOT.x} cy={PIVOT.y} r="65.4" fill="none" stroke="url(#tt-ring-light)" strokeWidth="1.2" />
      <circle cx={PIVOT.x} cy={PIVOT.y} r="55" fill="none" stroke="#a9adb7" strokeOpacity="0.6" strokeWidth="5" strokeDasharray="1 4.76" />
      <circle cx={PIVOT.x} cy={PIVOT.y} r="44" fill="url(#tt-aluminium)" />
      <circle cx={PIVOT.x} cy={PIVOT.y} r="43.4" fill="none" stroke="url(#tt-ring-light)" strokeWidth="1.2" />
      <circle cx={PIVOT.x} cy={PIVOT.y} r="31" fill="#17181d" />

      {/* Anti-skate dial */}
      <circle cx="904" cy="239" r="15" fill="#000" opacity="0.5" filter="url(#tt-blur-3)" />
      <circle cx="902" cy="236" r="14" fill="url(#tt-button)" />
      <circle cx="902" cy="236" r="13.4" fill="none" stroke="url(#tt-bevel)" strokeWidth="1.2" />
      <path d="M902 225v7" stroke="#d5d8de" strokeWidth="1.6" strokeLinecap="round" />

      {/* Arm rest and cue lever */}
      <g transform={`translate(${ARM_REST.x} ${ARM_REST.y}) rotate(${ARM_REST_ANGLE})`}>
        <circle cx="2" cy="4" r="15" fill="#000" opacity="0.5" filter="url(#tt-blur-3)" />
        <circle r="14" fill="url(#tt-button)" />
        <circle r="13.4" fill="none" stroke="url(#tt-bevel)" strokeWidth="1.2" />
        {/* The two prongs the arm drops between */}
        <rect x="-12.5" y="-8" width="5.5" height="16" rx="2.5" fill="#9a9ea8" />
        <rect x="7" y="-8" width="5.5" height="16" rx="2.5" fill="#9a9ea8" />
      </g>
      <RaisedRect x={896} y={396} width={30} height={52} radius={7} />
      <rect x="906.5" y="403" width="9" height="26" rx="4.5" fill="#b9bdc6" />
      <rect x="908" y="404.5" width="2.5" height="23" rx="1.25" fill="#f2f3f6" />

      {/* Start/stop and speed buttons */}
      <RaisedRect x={34} y={656} width={92} height={64} radius={9} />
      <text x="80" y="691" className="turntable-print" style={{ fontSize: 8 }}>
        START·STOP
      </text>
      <RaisedRect x={138} y={686} width={46} height={34} radius={6} />
      <circle cx="161" cy="696" r="8" fill="url(#tt-red-glow)" />
      <circle cx="161" cy="696" r="2.4" fill="#ff5a4d" />
      <text x="161" y="713" className="turntable-print">
        33
      </text>
      <RaisedRect x={190} y={686} width={46} height={34} radius={6} />
      <circle cx="213" cy="696" r="2.4" fill="#4a211f" />
      <text x="213" y="713" className="turntable-print">
        45
      </text>

      {/* Pitch fader */}
      <text x="948" y="474" className="turntable-print">
        PITCH
      </text>
      <rect x="942" y="486" width="12" height="210" rx="6" fill="#08090b" />
      <rect x="942.5" y="486.5" width="11" height="209" rx="5.5" fill="none" stroke="url(#tt-bevel)" strokeWidth="1" transform="rotate(180 948 591)" />
      {Array.from({ length: 17 }, (_, index) => (
        <path
          key={index}
          d={`M${index % 4 === 0 ? 915 : 921} ${494 + index * 12.125}H929`}
          stroke="#7d818d"
          strokeWidth="1"
        />
      ))}
      <circle cx="906" cy="591" r="9" fill="url(#tt-green-glow)" />
      <circle cx="906" cy="591" r="2.6" fill="#6dffa6" />
      <RaisedRect x={928} y={579} width={40} height={24} radius={5} />
      <path d="M933 591H963" stroke="#e6e8ec" strokeWidth="1.5" strokeLinecap="round" />
    </svg>
  );
});

/** The arm, drawn parked; `angle` swings it over the record. Drawn twice, once as its own shadow. */
const Tonearm = memo(function Tonearm({ angle, shadow = false }: { angle: number; shadow?: boolean }) {
  return (
    <svg
      className={`turntable-arm${shadow ? ' turntable-arm-shadow' : ''}`}
      viewBox={DECK}
      style={{ rotate: `${angle}deg` }}
      aria-hidden="true"
    >
      <g transform={`translate(${PIVOT.x} ${PIVOT.y})`}>
        {/* Counterweight, behind the pivot */}
        <path d="M0 0V-56" stroke="#676c76" strokeWidth="10" strokeLinecap="round" />
        <path d="M-1.6 0V-56" stroke="#e1e3e8" strokeWidth="3" />
        <rect x="-25" y="-116" width="50" height="62" rx="8" fill="url(#tt-weight)" />
        <rect x="-25" y="-71" width="50" height="13" fill="#000" opacity="0.3" />
        {Array.from({ length: 12 }, (_, index) => (
          <path key={index} d={`M${-22 + index * 4} -71v13`} stroke="#fff" strokeOpacity="0.28" strokeWidth="0.8" />
        ))}
        <rect x="-25" y="-116" width="50" height="62" rx="8" fill="none" stroke="#000" strokeOpacity="0.35" />

        {/* The tube: three strokes, dark to light, read as a round bar */}
        <path d={ARM_TUBE} fill="none" stroke="#5a5f69" strokeWidth="11" strokeLinecap="round" />
        <path d={ARM_TUBE} fill="none" stroke="#c3c7ce" strokeWidth="8" strokeLinecap="round" />
        <path d={ARM_TUBE} fill="none" stroke="#f7f8fa" strokeWidth="2.5" strokeLinecap="round" transform="translate(-1.8 0)" />

        <g transform={`translate(${HEADSHELL.x} ${HEADSHELL.y}) rotate(${HEADSHELL.angle})`}>
          <rect x="-7.5" y="-6" width="15" height="20" rx="3" fill="#9a9ea8" />
          <path d="M-7.5 1h15M-7.5 6h15" stroke="#5a5f69" strokeWidth="1.4" />
          <path
            d="M-9 14H9L15 30V72q0 5-5 5H-10q-5 0-5-5V30Z"
            fill="#202227"
            stroke="#5a5e69"
            strokeWidth="1.2"
            strokeLinejoin="round"
          />
          {/* Finger lift */}
          <path d="M15 36C27 36 33 30 42 22" fill="none" stroke="#b9bdc6" strokeWidth="4" strokeLinecap="round" />
          {HEADSHELL_WIRES.map(({ color, x }) => (
            <path key={color} d={`M${x} 17v9`} stroke={color} strokeWidth="1.6" strokeLinecap="round" />
          ))}
          <circle cx="-9" cy="34" r="2.3" fill="#b9bdc6" />
          <circle cx="9" cy="34" r="2.3" fill="#b9bdc6" />
          {/* Cartridge; the stylus is under its white front */}
          <rect x="-11" y="40" width="22" height="34" rx="3" fill="#0d0e11" stroke="#3a3d46" strokeWidth="1" />
          <rect x="-11" y="62" width="22" height="12" rx="3" fill="#ecedf1" />
        </g>

        <circle r="19" fill="url(#tt-dome)" />
        <circle r="18.5" fill="none" stroke="#000" strokeOpacity="0.3" />
        <circle r="6" fill="#2a2c33" />
      </g>
    </svg>
  );
});
