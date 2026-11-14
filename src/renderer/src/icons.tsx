import type { ReactNode } from 'react';

function Icon({ size = 16, children }: { size?: number; children: ReactNode }) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
      {children}
    </svg>
  );
}

export function PlayIcon({ size }: { size?: number }) {
  return (
    <Icon size={size}>
      <path d="M4 2.5v11l9-5.5z" />
    </Icon>
  );
}

export function PauseIcon({ size }: { size?: number }) {
  return (
    <Icon size={size}>
      <path d="M3.5 2.5h3v11h-3zM9.5 2.5h3v11h-3z" />
    </Icon>
  );
}

export function PreviousIcon({ size }: { size?: number }) {
  return (
    <Icon size={size}>
      <path d="M3 3h2v10H3zM13 3v10L6 8z" />
    </Icon>
  );
}

export function NextIcon({ size }: { size?: number }) {
  return (
    <Icon size={size}>
      <path d="M11 3h2v10h-2zM3 3v10l7-5z" />
    </Icon>
  );
}

export function VolumeIcon({ size }: { size?: number }) {
  return (
    <Icon size={size}>
      <path d="M2 6h2.5L8 3v10L4.5 10H2zM10 5.2a4 4 0 0 1 0 5.6l.9.9a5.3 5.3 0 0 0 0-7.4z" />
    </Icon>
  );
}

export function ChevronIcon({ size }: { size?: number }) {
  return (
    <Icon size={size}>
      <path d="M6 3.5 10.5 8 6 12.5 5 11.5 8.5 8 5 4.5z" />
    </Icon>
  );
}

export function FolderIcon({ size }: { size?: number }) {
  return (
    <Icon size={size}>
      <path d="M1.5 3.5h4.2l1.3 1.5h7.5v8h-13z" />
    </Icon>
  );
}

export function PlaylistIcon({ size }: { size?: number }) {
  return (
    <Icon size={size}>
      <path d="M2 3.5h12V5H2zM2 7.25h12v1.5H2zM2 11h8v1.5H2z" />
    </Icon>
  );
}
