//! Platter encoder and round key on the Raspberry Pi.
//!
//! The lines are named `GPIO17`, `GPIO27`, and `GPIO25` when the kernel
//! publishes those names. Otherwise the header is tried on `gpiochip0` and
//! then `gpiochip4`, which is where a Pi 5 kept it before the rename.
//! A machine with no GPIO chip leaves the keyboard working. A clockwise
//! detent, CLK falling first, moves the highlight down. If the platter
//! goes the wrong way, swap the CLK and DT wires.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use gpiocdev::line::{Bias, EdgeDetection, EdgeKind, Value};
use gpiocdev::Request;

use crate::input::{
    apply_level_change, Control, DeviceInput, Levels, Line, PLATTER_CLK, PLATTER_DT, SELECT_KEY,
};

/// How often a held key is checked so a press can finish debouncing.
const POLL: Duration = Duration::from_millis(5);

const CHIPS: [&str; 2] = ["/dev/gpiochip0", "/dev/gpiochip4"];

struct Lines {
    chip: PathBuf,
    clk: u32,
    dt: u32,
    key: u32,
}

/// Blocks until `stop` is set, sending one [`Control`] per turn or press.
pub(crate) fn run(mut emit: impl FnMut(Control), stop: &AtomicBool) {
    if let Err(error) = watch(&mut emit, stop, Instant::now()) {
        eprintln!("Platter input is unavailable: {error}");
    }
}

fn watch(
    emit: &mut impl FnMut(Control),
    stop: &AtomicBool,
    origin: Instant,
) -> Result<(), gpiocdev::Error> {
    let (request, lines) = open()?;
    let mut levels = read_levels(&request, &lines)?;
    let mut input = DeviceInput::new();
    input.seed(levels);

    while !stop.load(Ordering::Relaxed) {
        if request.wait_edge_event(POLL)? {
            while request.has_edge_event()? {
                let event = request.read_edge_event()?;
                note_edge(&lines, &mut levels, event.offset, event.kind);
                send_controls(emit, &mut input, levels, origin);
            }
        } else {
            send_controls(emit, &mut input, levels, origin);
        }
    }
    Ok(())
}

fn note_edge(lines: &Lines, levels: &mut Levels, offset: u32, kind: EdgeKind) {
    let rose = kind == EdgeKind::Rising;
    if offset == lines.clk {
        apply_level_change(levels, Line::Clk, rose);
    } else if offset == lines.dt {
        apply_level_change(levels, Line::Dt, rose);
    } else if offset == lines.key {
        apply_level_change(levels, Line::Key, rose);
    }
}

fn send_controls(
    emit: &mut impl FnMut(Control),
    input: &mut DeviceInput,
    levels: Levels,
    origin: Instant,
) {
    for control in input.sample(levels, origin.elapsed()) {
        emit(control);
    }
}

fn open() -> Result<(Request, Lines), gpiocdev::Error> {
    if let Some(lines) = named_lines() {
        let request = request_on(&lines)?;
        return Ok((request, lines));
    }

    let mut last = None;
    for chip in CHIPS {
        let lines = Lines {
            chip: PathBuf::from(chip),
            clk: PLATTER_CLK,
            dt: PLATTER_DT,
            key: SELECT_KEY,
        };
        match request_on(&lines) {
            Ok(request) => return Ok((request, lines)),
            Err(error) => last = Some(error),
        }
    }
    Err(last.unwrap_or(gpiocdev::Error::NoGpioChips()))
}

fn named_lines() -> Option<Lines> {
    let clk = gpiocdev::find_named_line("GPIO17")?;
    let dt = gpiocdev::find_named_line("GPIO27")?;
    let key = gpiocdev::find_named_line("GPIO25")?;
    if clk.chip != dt.chip || clk.chip != key.chip {
        return None;
    }
    Some(Lines {
        chip: clk.chip,
        clk: clk.info.offset,
        dt: dt.info.offset,
        key: key.info.offset,
    })
}

fn request_on(lines: &Lines) -> Result<Request, gpiocdev::Error> {
    gpiocdev::Request::builder()
        .on_chip(&lines.chip)
        .with_consumer("djusb-device")
        .as_input()
        .with_bias(Bias::PullUp)
        .with_lines(&[lines.clk, lines.dt])
        .with_edge_detection(EdgeDetection::BothEdges)
        .with_line(lines.key)
        .with_edge_detection(EdgeDetection::BothEdges)
        .request()
}

fn read_levels(request: &Request, lines: &Lines) -> Result<Levels, gpiocdev::Error> {
    Ok(Levels {
        clk_high: request.value(lines.clk)? == Value::Active,
        dt_high: request.value(lines.dt)? == Value::Active,
        switch_pressed: request.value(lines.key)? == Value::Inactive,
    })
}
