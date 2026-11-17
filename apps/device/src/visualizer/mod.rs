//! The waveform, the equalizer, the CDJ deck, and MilkDrop.
//!
//! The band maths and the oscilloscope trace are the desktop's
//! `equalizerBands.ts` and `Waveform.tsx`. The spectrum is an FFT of the
//! audio copy. The deck reads the file for its waveform and the playhead
//! for the time. MilkDrop is projectM, loaded as a shared library.

mod bands;
mod deck;
mod deck_math;
mod draw;
mod meter;
mod milk;
mod peaks;
mod spectrum;
mod waveform;

pub(crate) use deck::{deck, DeckTrack};
pub(crate) use draw::{equalizer, waveform, EqualizerTheme, WaveformTheme};
pub(crate) use milk::milkdrop;
