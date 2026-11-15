/** Builds Serato `database V2` and `.crate` files for tests. */

export function chunk(tag: string, data: Buffer): Buffer {
  const header = Buffer.alloc(8);
  header.write(tag, 0, 'latin1');
  header.writeUInt32BE(data.length, 4);
  return Buffer.concat([header, data]);
}

export function textChunk(tag: string, text: string): Buffer {
  return chunk(tag, Buffer.from(text, 'utf16le').swap16());
}

/** Each track maps field tags such as `pfil` or `tsng` to their text. */
export function seratoDatabase(tracks: Record<string, string>[]): Buffer {
  return Buffer.concat([
    textChunk('vrsn', '2.0/Serato Scratch LIVE Database'),
    ...tracks.map((fields) =>
      chunk(
        'otrk',
        Buffer.concat([
          ...Object.entries(fields).map(([tag, text]) => textChunk(tag, text)),
          // Real records also carry binary fields.
          chunk('uadd', Buffer.from([0x5e, 0x3c, 0x07, 0xea])),
          chunk('bmis', Buffer.from([0])),
        ]),
      ),
    ),
  ]);
}

export function seratoCrate(trackPaths: string[]): Buffer {
  return Buffer.concat([
    textChunk('vrsn', '1.0/Serato ScratchLive Crate'),
    chunk('osrt', Buffer.concat([textChunk('tvcn', '#'), chunk('brev', Buffer.from([0]))])),
    chunk('ovct', Buffer.concat([textChunk('tvcn', 'song'), textChunk('tvcw', '0')])),
    ...trackPaths.map((trackPath) => chunk('otrk', textChunk('ptrk', trackPath))),
  ]);
}
