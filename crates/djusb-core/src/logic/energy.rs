//! Harmonic mixing and the energy-flow chart.

use crate::logic::key::{camelot_key, CamelotKey, CamelotLetter};

/// How well two keys mix on the Camelot wheel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HarmonicKind {
    Smooth,
    Shift,
    Clash,
}

/// A mix from one key into another.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HarmonicMix {
    pub kind: HarmonicKind,
    /// What the move is called, for tooltips.
    pub label: &'static str,
}

/// More of a tempo change than this between two tracks is hard to beatmatch.
pub const TEMPO_JUMP: f64 = 0.06;

/// The tempo a track would be played at after another.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TempoMatch {
    pub bpm: f64,
    pub half_time: bool,
}

/// One track on the energy-flow chart.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlowPoint {
    pub key: Option<CamelotKey>,
    pub bpm: Option<f64>,
    /// Height on the chart in BPM: usually the track's BPM, but doubled or
    /// halved when that is how it would be mixed with the track before it.
    pub level: Option<f64>,
}

/// The mix between one track and the next.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlowMix {
    /// `None` when either key is unknown.
    pub harmonic: Option<HarmonicMix>,
    /// Relative tempo change, allowing for half and double time.
    /// `None` when either BPM is unknown.
    pub tempo_change: Option<f64>,
    pub tempo_jump: bool,
    /// The tempos only line up at half or double time.
    pub half_time: bool,
}

/// How many mixes of each kind a list contains.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HarmonicCounts {
    pub smooth: usize,
    pub shift: usize,
    pub clash: usize,
}

/// The energy flow of a track list, in list order.
#[derive(Debug, Clone, PartialEq)]
pub struct EnergyFlow {
    /// One per track.
    pub points: Vec<FlowPoint>,
    /// Between each track and the next.
    pub mixes: Vec<FlowMix>,
    pub counts: HarmonicCounts,
    pub tempo_jumps: usize,
}

/// A track the energy-flow chart can read. [`crate::Track`] implements this.
pub trait FlowTrack {
    fn flow_bpm(&self) -> Option<f64>;
    fn flow_key(&self) -> Option<&str>;
}

impl FlowTrack for crate::Track {
    fn flow_bpm(&self) -> Option<f64> {
        self.bpm
    }

    fn flow_key(&self) -> Option<&str> {
        self.key.as_deref()
    }
}

impl FlowTrack for (Option<f64>, Option<&str>) {
    fn flow_bpm(&self) -> Option<f64> {
        self.0
    }

    fn flow_key(&self) -> Option<&str> {
        self.1
    }
}

/// Steps from one wheel number to another the short way round, from -5 to 6.
fn wheel_steps(from: u8, to: u8) -> i32 {
    let steps = (i32::from(to) - i32::from(from)).rem_euclid(12);
    if steps > 6 {
        steps - 12
    } else {
        steps
    }
}

/// Rates a mix from one key into another by the usual Camelot wheel rules.
#[must_use]
pub fn classify_harmonic(from: CamelotKey, to: CamelotKey) -> HarmonicMix {
    let steps = wheel_steps(from.number, to.number);
    if from.letter == to.letter {
        return match steps {
            0 => mix(HarmonicKind::Smooth, "Same key"),
            1 => mix(HarmonicKind::Smooth, "One step up"),
            -1 => mix(HarmonicKind::Smooth, "One step down"),
            2 => mix(HarmonicKind::Shift, "Energy boost"),
            -2 => mix(HarmonicKind::Shift, "Energy drop"),
            // Seven steps round the wheel is a semitone up; five is a semitone down.
            -5 => mix(HarmonicKind::Shift, "Semitone up"),
            5 => mix(HarmonicKind::Shift, "Semitone down"),
            _ => mix(HarmonicKind::Clash, "Key clash"),
        };
    }
    if steps == 0 {
        return mix(
            HarmonicKind::Smooth,
            if to.letter == CamelotLetter::B {
                "Relative major"
            } else {
                "Relative minor"
            },
        );
    }
    // Minor up one into major, or major down one into minor, keeps most notes in common.
    if (from.letter == CamelotLetter::A && steps == 1)
        || (from.letter == CamelotLetter::B && steps == -1)
    {
        return mix(HarmonicKind::Shift, "Diagonal mix");
    }
    mix(HarmonicKind::Clash, "Key clash")
}

fn mix(kind: HarmonicKind, label: &'static str) -> HarmonicMix {
    HarmonicMix { kind, label }
}

/// The tempo `to` would be played at after `from`: as it is, or doubled or
/// halved when only that brings it within beatmatching range.
#[must_use]
pub fn match_tempo(from: f64, to: f64) -> TempoMatch {
    let change = |bpm: f64| (bpm - from).abs() / from;
    if change(to) <= TEMPO_JUMP {
        return TempoMatch {
            bpm: to,
            half_time: false,
        };
    }
    for bpm in [to * 2.0, to / 2.0] {
        if change(bpm) <= TEMPO_JUMP {
            return TempoMatch {
                bpm,
                half_time: true,
            };
        }
    }
    TempoMatch {
        bpm: to,
        half_time: false,
    }
}

#[must_use]
pub fn energy_flow(tracks: &[impl FlowTrack]) -> EnergyFlow {
    let mut counts = HarmonicCounts {
        smooth: 0,
        shift: 0,
        clash: 0,
    };
    let mut tempo_jumps = 0;
    let mut previous_level: Option<f64> = None;
    let points = tracks
        .iter()
        .map(|track| {
            let key = track
                .flow_key()
                .filter(|key| !key.is_empty())
                .and_then(camelot_key);
            let bpm = track.flow_bpm().filter(|bpm| *bpm > 0.0);
            let mut level = bpm;
            if let (Some(bpm), Some(previous)) = (bpm, previous_level) {
                level = Some(match_tempo(previous, bpm).bpm);
            }
            if let Some(level) = level {
                previous_level = Some(level);
            }
            FlowPoint { key, bpm, level }
        })
        .collect::<Vec<_>>();

    let mixes = points
        .windows(2)
        .map(|pair| {
            let previous = pair[0];
            let point = pair[1];
            let harmonic = match (previous.key, point.key) {
                (Some(from), Some(to)) => Some(classify_harmonic(from, to)),
                _ => None,
            };
            if let Some(harmonic) = harmonic {
                match harmonic.kind {
                    HarmonicKind::Smooth => counts.smooth += 1,
                    HarmonicKind::Shift => counts.shift += 1,
                    HarmonicKind::Clash => counts.clash += 1,
                }
            }
            let (Some(from_bpm), Some(to_bpm)) = (previous.bpm, point.bpm) else {
                return FlowMix {
                    harmonic,
                    tempo_change: None,
                    tempo_jump: false,
                    half_time: false,
                };
            };
            let matched = match_tempo(from_bpm, to_bpm);
            let tempo_change = (matched.bpm - from_bpm) / from_bpm;
            let tempo_jump = tempo_change.abs() > TEMPO_JUMP;
            if tempo_jump {
                tempo_jumps += 1;
            }
            FlowMix {
                harmonic,
                tempo_change: Some(tempo_change),
                tempo_jump,
                half_time: matched.half_time,
            }
        })
        .collect();

    EnergyFlow {
        points,
        mixes,
        counts,
        tempo_jumps,
    }
}

/// Evenly spaced BPM gridlines covering a range, about four of them.
#[must_use]
pub fn bpm_grid(low: f64, high: f64) -> Vec<f64> {
    let step = [1.0, 2.0, 5.0, 10.0, 20.0, 50.0]
        .into_iter()
        .find(|candidate| (high - low) / candidate <= 5.0)
        .unwrap_or(100.0);
    let mut lines = Vec::new();
    let mut bpm = (low / step).ceil() * step;
    while bpm <= high {
        lines.push(bpm);
        bpm += step;
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::{
        bpm_grid, classify_harmonic, energy_flow, match_tempo, HarmonicCounts, HarmonicKind,
        TempoMatch,
    };
    use crate::logic::camelot_key;

    fn kind(from: &str, to: &str) -> HarmonicKind {
        classify_harmonic(camelot_key(from).unwrap(), camelot_key(to).unwrap()).kind
    }

    #[test]
    fn rates_neighbouring_and_relative_keys_as_smooth() {
        assert_eq!(kind("8A", "8A"), HarmonicKind::Smooth);
        assert_eq!(kind("8A", "9A"), HarmonicKind::Smooth);
        assert_eq!(kind("1B", "12B"), HarmonicKind::Smooth);
        assert_eq!(kind("8A", "8B"), HarmonicKind::Smooth);
        assert_eq!(
            classify_harmonic(camelot_key("8A").unwrap(), camelot_key("9A").unwrap()).label,
            "One step up"
        );
        assert_eq!(
            classify_harmonic(camelot_key("8A").unwrap(), camelot_key("8B").unwrap()).label,
            "Relative major"
        );
    }

    #[test]
    fn rates_energy_boosts_semitone_changes_and_diagonal_mixes_as_shifts() {
        assert_eq!(kind("8A", "10A"), HarmonicKind::Shift);
        assert_eq!(kind("12A", "2A"), HarmonicKind::Shift);
        assert_eq!(kind("8A", "3A"), HarmonicKind::Shift);
        assert_eq!(kind("8A", "9B"), HarmonicKind::Shift);
        assert_eq!(kind("8B", "7A"), HarmonicKind::Shift);
        assert_eq!(
            classify_harmonic(camelot_key("8A").unwrap(), camelot_key("3A").unwrap()).label,
            "Semitone up"
        );
        assert_eq!(
            classify_harmonic(camelot_key("8A").unwrap(), camelot_key("9B").unwrap()).label,
            "Diagonal mix"
        );
    }

    #[test]
    fn rates_anything_else_as_a_clash() {
        assert_eq!(kind("8A", "11A"), HarmonicKind::Clash);
        assert_eq!(kind("8A", "7B"), HarmonicKind::Clash);
        assert_eq!(kind("8B", "2A"), HarmonicKind::Clash);
    }

    #[test]
    fn keeps_tempos_that_are_close_enough_to_beatmatch() {
        assert_eq!(
            match_tempo(124.0, 128.0),
            TempoMatch {
                bpm: 128.0,
                half_time: false
            }
        );
    }

    #[test]
    fn doubles_or_halves_a_tempo_when_only_that_lines_it_up() {
        assert_eq!(
            match_tempo(172.0, 87.0),
            TempoMatch {
                bpm: 174.0,
                half_time: true
            }
        );
        assert_eq!(
            match_tempo(70.0, 140.0),
            TempoMatch {
                bpm: 70.0,
                half_time: true
            }
        );
    }

    #[test]
    fn leaves_tempos_that_do_not_line_up_either_way() {
        assert_eq!(
            match_tempo(100.0, 128.0),
            TempoMatch {
                bpm: 128.0,
                half_time: false
            }
        );
    }

    #[test]
    fn rates_each_mix_by_key_and_flags_tempo_jumps() {
        let flow = energy_flow(&[
            (Some(124.0), Some("8A")),
            (Some(126.0), Some("9A")),
            (Some(140.0), Some("12A")),
        ]);
        assert_eq!(
            flow.mixes
                .iter()
                .map(|mix| mix.harmonic.unwrap().kind)
                .collect::<Vec<_>>(),
            [HarmonicKind::Smooth, HarmonicKind::Clash]
        );
        assert_eq!(
            flow.mixes
                .iter()
                .map(|mix| mix.tempo_jump)
                .collect::<Vec<_>>(),
            [false, true]
        );
        let change = flow.mixes[0].tempo_change.unwrap();
        assert!((change - 2.0 / 124.0).abs() < 1e-12, "{change}");
        assert_eq!(
            flow.counts,
            HarmonicCounts {
                smooth: 1,
                shift: 0,
                clash: 1
            }
        );
        assert_eq!(flow.tempo_jumps, 1);
    }

    #[test]
    fn plots_a_half_time_track_at_the_tempo_it_would_be_mixed_at() {
        let flow = energy_flow(&[(Some(172.0), None), (Some(87.0), None)]);
        assert_eq!(
            flow.points
                .iter()
                .map(|point| point.level)
                .collect::<Vec<_>>(),
            [Some(172.0), Some(174.0)]
        );
        assert_eq!(flow.points[1].bpm, Some(87.0));
        assert!(!flow.mixes[0].tempo_jump);
        assert!(flow.mixes[0].half_time);
    }

    #[test]
    fn leaves_mixes_next_to_a_track_without_a_bpm_or_key_unrated() {
        let flow = energy_flow(&[
            (Some(124.0), Some("8A")),
            (None, Some("nonsense")),
            (Some(125.0), Some("8A")),
        ]);
        assert_eq!(
            flow.points
                .iter()
                .map(|point| point.level)
                .collect::<Vec<_>>(),
            [Some(124.0), None, Some(125.0)]
        );
        assert_eq!(flow.mixes.len(), 2);
        for mix in &flow.mixes {
            assert!(mix.harmonic.is_none());
            assert!(mix.tempo_change.is_none());
            assert!(!mix.tempo_jump);
            assert!(!mix.half_time);
        }
    }

    #[test]
    fn picks_a_round_step_for_the_range() {
        assert_eq!(
            bpm_grid(121.5, 130.5),
            vec![122.0, 124.0, 126.0, 128.0, 130.0]
        );
        assert_eq!(bpm_grid(85.0, 175.0), vec![100.0, 120.0, 140.0, 160.0]);
    }
}
