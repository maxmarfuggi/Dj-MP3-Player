import type { ReactNode } from 'react';

interface IconProps {
  size?: number;
}

/** A solid glyph on a 16×16 grid. */
function Icon({ size = 16, children }: IconProps & { children: ReactNode }) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
      {children}
    </svg>
  );
}

/** An outline glyph on a 16×16 grid. */
function LineIcon({ size = 16, children }: IconProps & { children: ReactNode }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.5"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      {children}
    </svg>
  );
}

export function PlayIcon({ size }: IconProps) {
  return (
    <Icon size={size}>
      <path d="M4.5 2.9c0-.6.65-.97 1.16-.65l8 5.1c.47.3.47.99 0 1.3l-8 5.1c-.51.32-1.16-.05-1.16-.65z" />
    </Icon>
  );
}

export function PauseIcon({ size }: IconProps) {
  return (
    <Icon size={size}>
      <rect x="3.25" y="2.5" width="3.5" height="11" rx="1" />
      <rect x="9.25" y="2.5" width="3.5" height="11" rx="1" />
    </Icon>
  );
}

export function PreviousIcon({ size }: IconProps) {
  return (
    <Icon size={size}>
      <rect x="2.5" y="3" width="2" height="10" rx="0.9" />
      <path d="M13.5 3.75v8.5c0 .6-.66.96-1.16.63l-6.4-4.25a.75.75 0 0 1 0-1.26l6.4-4.25c.5-.33 1.16.03 1.16.63z" />
    </Icon>
  );
}

export function NextIcon({ size }: IconProps) {
  return (
    <Icon size={size}>
      <rect x="11.5" y="3" width="2" height="10" rx="0.9" />
      <path d="M2.5 3.75v8.5c0 .6.66.96 1.16.63l6.4-4.25a.75.75 0 0 0 0-1.26l-6.4-4.25c-.5-.33-1.16.03-1.16.63z" />
    </Icon>
  );
}

/** A speaker with more sound waves the louder it is, or a cross when silent. */
export function VolumeIcon({ size, volume = 1 }: IconProps & { volume?: number }) {
  return (
    <LineIcon size={size}>
      <path d="M2.25 6.25h2.1l3.4-2.8v9.1l-3.4-2.8h-2.1z" fill="currentColor" />
      {volume === 0 ? (
        <path d="m10.75 6.25 3.25 3.5M14 6.25l-3.25 3.5" />
      ) : (
        <>
          <path d="M10.6 5.9a3 3 0 0 1 0 4.2" />
          {volume > 0.5 && <path d="M12.5 4a5.6 5.6 0 0 1 0 8" />}
        </>
      )}
    </LineIcon>
  );
}

export function ChevronIcon({ size }: IconProps) {
  return (
    <LineIcon size={size}>
      <path d="M6 3.75 10.25 8 6 12.25" />
    </LineIcon>
  );
}

export function FolderIcon({ size }: IconProps) {
  return (
    <LineIcon size={size}>
      <path d="M1.75 4.5c0-.7.56-1.25 1.25-1.25h2.8c.33 0 .65.13.88.37l.9.88H13c.7 0 1.25.56 1.25 1.25v5.5c0 .7-.56 1.25-1.25 1.25H3c-.7 0-1.25-.56-1.25-1.25z" />
    </LineIcon>
  );
}

export function PlaylistIcon({ size }: IconProps) {
  return (
    <LineIcon size={size}>
      <path d="M2.5 4.25h11M2.5 8h11M2.5 11.75h6.5" />
    </LineIcon>
  );
}

export function DiscIcon({ size }: IconProps) {
  return (
    <LineIcon size={size}>
      <circle cx="8" cy="8" r="6.25" />
      <circle cx="8" cy="8" r="1.75" />
    </LineIcon>
  );
}

export function UsbIcon({ size }: IconProps) {
  return (
    <LineIcon size={size}>
      <path d="M4.75 6.75h6.5v6c0 .83-.67 1.5-1.5 1.5h-3.5c-.83 0-1.5-.67-1.5-1.5zM6 6.75v-5h4v5M7.25 3.9v.6M8.75 3.9v.6" />
    </LineIcon>
  );
}

export function PlusIcon({ size }: IconProps) {
  return (
    <LineIcon size={size}>
      <path d="M8 3.25v9.5M3.25 8h9.5" />
    </LineIcon>
  );
}

export function CloseIcon({ size }: IconProps) {
  return (
    <LineIcon size={size}>
      <path d="m4 4 8 8M12 4l-8 8" />
    </LineIcon>
  );
}

export function WarningIcon({ size }: IconProps) {
  return (
    <LineIcon size={size}>
      <path d="M8 2.4 14.4 13.25H1.6zM8 6.75v3M8 11.3v.01" />
    </LineIcon>
  );
}

export function SortArrowIcon({ size, direction }: IconProps & { direction: 'asc' | 'desc' }) {
  return (
    <LineIcon size={size}>
      {direction === 'asc' ? (
        <path d="M8 12.75v-9.5M4.25 7 8 3.25 11.75 7" />
      ) : (
        <path d="M8 3.25v9.5M4.25 9 8 12.75 11.75 9" />
      )}
    </LineIcon>
  );
}

export function VisualizerIcon({ size }: IconProps) {
  return (
    <Icon size={size}>
      <rect x="1.5" y="7" width="1.6" height="2" rx="0.6" />
      <rect x="4.3" y="4.5" width="1.6" height="7" rx="0.6" />
      <rect x="7.2" y="2" width="1.6" height="12" rx="0.6" />
      <rect x="10.1" y="5" width="1.6" height="6" rx="0.6" />
      <rect x="12.9" y="6.5" width="1.6" height="3" rx="0.6" />
    </Icon>
  );
}
