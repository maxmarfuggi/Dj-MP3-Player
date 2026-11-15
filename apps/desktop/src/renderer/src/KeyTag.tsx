import { type CSSProperties, memo } from 'react';
import { keyHue } from './keyColor';

/** A track's musical key, tinted with its Camelot wheel colour when it is a known key. */
export const KeyTag = memo(function KeyTag({ musicalKey }: { musicalKey: string }) {
  const hue = keyHue(musicalKey);
  return (
    <span
      className={`key-tag${hue === null ? '' : ' key-tag-colored'}`}
      style={hue === null ? undefined : ({ '--key-hue': hue } as CSSProperties)}
    >
      {musicalKey}
    </span>
  );
});
