//! Speakers the pairing list can show, and the audio output that belongs to one.
//!
//! The Linux session scans and connects. These functions decide what that
//! session is allowed to put on screen, so the list can be checked without
//! a radio.

use djusb_core::BluetoothSpeaker;

/// One speaker on the pairing list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Speaker {
    pub address: String,
    pub name: String,
    pub paired: bool,
    pub connected: bool,
    /// A connect requested from the list is still running.
    pub busy: bool,
    /// Why the last connect failed. Empty while connecting or once it works.
    pub problem: Option<String>,
}

impl Speaker {
    /// The words beside the name. Connecting wins over a leftover failure.
    #[must_use]
    pub fn detail(&self) -> String {
        if self.busy {
            return "Connecting".to_string();
        }
        if let Some(problem) = self.problem.as_ref().filter(|text| !text.is_empty()) {
            return problem.clone();
        }
        if self.connected {
            return "Connected".to_string();
        }
        if self.paired {
            return "Paired".to_string();
        }
        "Available".to_string()
    }
}

/// What BlueZ has said about a device, before it is accepted as a speaker.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
#[derive(Debug, Clone)]
pub struct Heard {
    pub class: Option<u32>,
    pub icon: Option<String>,
    pub uuids: Vec<String>,
    pub paired: bool,
    pub connected: bool,
    /// The radio has heard it recently.
    pub present: bool,
}

/// Audio devices that are paired, connected, or in range.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
#[must_use]
pub fn show_speaker(heard: &Heard) -> bool {
    if !is_audio(heard) {
        return false;
    }
    heard.paired || heard.connected || heard.present
}

/// A loudspeaker, headphones, or a headset. Phones and keyboards are not.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn is_audio(heard: &Heard) -> bool {
    if heard.class.is_some_and(|class| class & 0x1f00 == 0x0400) {
        return true;
    }
    if let Some(icon) = &heard.icon {
        let icon = icon.to_ascii_lowercase();
        if icon.contains("audio") || icon.contains("speaker") || icon.contains("headset") {
            return true;
        }
    }
    // A2DP sink, headset, and hands-free. An audio source is a phone, so it is left out.
    heard.uuids.iter().any(|uuid| {
        let uuid = uuid.to_ascii_lowercase();
        uuid.contains("0000110b") || uuid.contains("00001108") || uuid.contains("0000111e")
    })
}

/// What the pairing screen asks the radio to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Look for speakers while the pairing list is open.
    Scan(bool),
    /// Pair if needed, connect, trust, and make this the audio output.
    Connect(String),
}

/// News from the radio.
#[derive(Debug, Clone)]
pub enum Event {
    /// The radio is listening. Commands sent here are handled in order.
    Ready(iced::futures::channel::mpsc::UnboundedSender<Command>),
    /// The adapter and the speakers to show.
    Snapshot(Snapshot),
    /// A connect finished. The player remembers this speaker and reopens audio.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    Chosen(Chosen),
}

/// Whether a radio is present, and the speakers it currently knows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub adapter: bool,
    pub speakers: Vec<Speaker>,
}

impl Snapshot {
    fn off() -> Self {
        Self {
            adapter: false,
            speakers: Vec::new(),
        }
    }
}

/// A speaker that connected and should be the one used after the next boot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chosen {
    pub address: String,
    pub name: String,
}

/// The Bluetooth task. On Linux it talks to BlueZ. Elsewhere it reports no adapter.
pub fn feed() -> impl iced::futures::Stream<Item = Event> {
    iced::stream::channel(8, async |mut output| {
        use iced::futures::SinkExt;
        use iced::futures::StreamExt;

        let (commands, mut incoming) = iced::futures::channel::mpsc::unbounded();
        let _ = output.send(Event::Ready(commands)).await;
        #[cfg(target_os = "linux")]
        radio::serve(&mut incoming, &mut output).await;
        #[cfg(not(target_os = "linux"))]
        {
            let _ = output.send(Event::Snapshot(Snapshot::off())).await;
            while incoming.next().await.is_some() {}
        }
    })
}

/// How long to leave a speaker alone after a connect attempt.
pub const RECONNECT_AFTER: std::time::Duration = std::time::Duration::from_secs(5);

/// A remembered speaker is tried again once the last attempt is old enough
/// and nothing is connecting. `done` means it is connected and already the output.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
#[must_use]
pub fn due_reconnect(done: bool, busy: bool, since: std::time::Duration) -> bool {
    !done && !busy && since >= RECONNECT_AFTER
}

/// The sink id or name in a `wpctl` or `pactl` listing for this address.
///
/// A `wpctl` line yields the numeric id. A `pactl` short line yields the sink
/// name. Lines for other devices are ignored.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
#[must_use]
pub fn sink_for_address(listing: &str, address: &str) -> Option<String> {
    let needle = BluetoothSpeaker::new(address, "")?
        .address()
        .replace(':', "_");
    for line in listing.lines() {
        if !line.to_ascii_uppercase().contains(&needle) {
            continue;
        }
        if !line.to_ascii_lowercase().contains("bluez") {
            continue;
        }
        if let Some((_, rest)) = line.split_once('\t') {
            let name = rest.split('\t').next().unwrap_or("").trim();
            if !name.is_empty() {
                return Some(name.to_string());
            }
        }
        let id = line.split_whitespace().find_map(|token| {
            let bare = token.trim_end_matches(['.', ':']);
            bare.chars()
                .all(|ch| ch.is_ascii_digit())
                .then_some(bare)
                .filter(|bare| !bare.is_empty())
        });
        if let Some(id) = id {
            return Some(id.to_string());
        }
    }
    None
}

/// BlueZ on the system bus, and PipeWire or Pulse as the default output.
#[cfg(target_os = "linux")]
mod radio {
    use std::collections::HashMap;
    use std::process::Command;
    use std::time::Instant;

    use bluer::agent::{Agent, AgentHandle};
    use bluer::{Adapter, Address, Session};
    use djusb_core::BluetoothSpeaker;
    use iced::futures::channel::mpsc::{Sender, UnboundedReceiver};
    use iced::futures::{SinkExt, StreamExt};
    use tokio::task::JoinHandle;

    use super::{
        due_reconnect, show_speaker, sink_for_address, Chosen, Command, Event, Heard, Snapshot,
        Speaker,
    };

    /// How long to wait for the speaker to show up as an output.
    const OUTPUT_TRIES: usize = 8;

    struct Radio {
        adapter: Adapter,
        devices: HashMap<Address, Speaker>,
        discovery: Option<JoinHandle<()>>,
        scanning: bool,
        /// The speaker to bring back after a failed attempt or the next boot.
        remembered: Option<String>,
        /// Set once that speaker is connected and is the default output.
        routed: Option<String>,
        last_attempt: Instant,
        busy: Option<String>,
        problem: Option<(String, String)>,
    }

    pub async fn serve(commands: &mut UnboundedReceiver<Command>, output: &mut Sender<Event>) {
        let session = match Session::new().await {
            Ok(session) => session,
            Err(error) => {
                eprintln!("Bluetooth is unavailable: {error}");
                let _ = output.send(Event::Snapshot(Snapshot::off())).await;
                while commands.next().await.is_some() {}
                return;
            }
        };
        let adapter = match session.default_adapter().await {
            Ok(adapter) => adapter,
            Err(error) => {
                eprintln!("No Bluetooth adapter: {error}");
                let _ = output.send(Event::Snapshot(Snapshot::off())).await;
                while commands.next().await.is_some() {}
                return;
            }
        };
        if let Err(error) = adapter.set_powered(true).await {
            eprintln!("Bluetooth could not be switched on: {error}");
        }
        // Kept for the life of the player so pairing confirmation has somewhere to go.
        let _agent = register_agent(&session).await;

        let mut radio = Radio {
            adapter,
            devices: HashMap::new(),
            discovery: None,
            scanning: false,
            remembered: None,
            routed: None,
            last_attempt: Instant::now(),
            busy: None,
            problem: None,
        };
        radio.refresh().await;
        radio.publish(output).await;

        loop {
            tokio::select! {
                command = commands.next() => {
                    let Some(command) = command else { break };
                    radio.handle(command, output).await;
                }
                () = tokio::time::sleep(std::time::Duration::from_secs(1)) => {
                    radio.maintain(output).await;
                }
            }
        }
    }

    async fn register_agent(session: &Session) -> Option<AgentHandle> {
        match session
            .register_agent(Agent {
                request_default: true,
                ..Agent::default()
            })
            .await
        {
            Ok(handle) => Some(handle),
            Err(error) => {
                eprintln!("Bluetooth agent is not the default: {error}");
                session.register_agent(Agent::default()).await.ok()
            }
        }
    }

    impl Radio {
        async fn handle(&mut self, command: Command, output: &mut Sender<Event>) {
            match command {
                Command::Scan(on) => self.set_scan(on, output).await,
                Command::Connect(address) => self.connect(address, output).await,
            }
        }

        async fn set_scan(&mut self, on: bool, output: &mut Sender<Event>) {
            self.scanning = on;
            self.sync_discovery();
            self.publish(output).await;
        }

        fn start_discovery(&mut self) {
            if self.discovery.is_some() {
                return;
            }
            let adapter = self.adapter.clone();
            self.discovery = Some(tokio::spawn(async move {
                match adapter.discover_devices().await {
                    Ok(found) => {
                        let mut found = std::pin::pin!(found);
                        while found.next().await.is_some() {}
                    }
                    Err(error) => eprintln!("Bluetooth scan could not start: {error}"),
                }
            }));
        }

        fn stop_discovery(&mut self) {
            if let Some(task) = self.discovery.take() {
                task.abort();
            }
        }

        async fn connect(&mut self, address: String, output: &mut Sender<Event>) {
            let Some(speaker) = BluetoothSpeaker::new(&address, "") else {
                eprintln!("Bluetooth address is not usable: {address}");
                return;
            };
            let address = speaker.address().to_string();
            self.remembered = Some(address.clone());
            // Inquiry makes a poor moment to open an audio link.
            self.stop_discovery();
            self.busy = Some(address.clone());
            self.problem = None;
            self.publish(output).await;

            match connect_device(&self.adapter, &address).await {
                Ok(name) => {
                    self.routed = Some(address.clone());
                    let _ = output
                        .send(Event::Chosen(Chosen {
                            address: address.clone(),
                            name,
                        }))
                        .await;
                }
                Err(message) => {
                    eprintln!("Bluetooth could not connect to {address}: {message}");
                    self.problem = Some((address, message));
                }
            }
            self.busy = None;
            self.last_attempt = Instant::now();
            self.refresh().await;
            self.publish(output).await;
            self.sync_discovery();
        }

        /// Looks again for the remembered speaker, and connects it when it is quiet.
        async fn maintain(&mut self, output: &mut Sender<Event>) {
            self.refresh().await;
            if let Some(address) = self.remembered.clone() {
                if !self.is_connected(&address) {
                    self.routed = None;
                }
            }
            self.sync_discovery();
            if let Some(address) = self.remembered.clone() {
                let routed = self
                    .routed
                    .as_ref()
                    .is_some_and(|routed| routed.eq_ignore_ascii_case(&address));
                let done = routed && self.is_connected(&address);
                if self.knows(&address)
                    && due_reconnect(done, self.busy.is_some(), self.last_attempt.elapsed())
                {
                    self.connect(address, output).await;
                    return;
                }
            }
            self.publish(output).await;
        }

        fn knows(&self, address: &str) -> bool {
            self.devices
                .values()
                .any(|speaker| speaker.address.eq_ignore_ascii_case(address))
        }

        fn is_connected(&self, address: &str) -> bool {
            self.devices
                .values()
                .any(|speaker| speaker.address.eq_ignore_ascii_case(address) && speaker.connected)
        }

        /// The pairing list wants a scan, or the remembered speaker is not known yet.
        fn discovery_wanted(&self) -> bool {
            if self.scanning {
                return true;
            }
            let Some(address) = self.remembered.clone() else {
                return false;
            };
            !self.knows(&address)
        }

        fn sync_discovery(&mut self) {
            if self.discovery_wanted() {
                self.start_discovery();
            } else {
                self.stop_discovery();
            }
        }

        async fn refresh(&mut self) {
            let Ok(addresses) = self.adapter.device_addresses().await else {
                return;
            };
            self.devices.clear();
            for address in addresses {
                if let Some(speaker) = read_speaker(&self.adapter, address).await {
                    self.devices.insert(address, speaker);
                }
            }
        }

        async fn publish(&mut self, output: &mut Sender<Event>) {
            let _ = output
                .send(Event::Snapshot(Snapshot {
                    adapter: true,
                    speakers: listed(&self.devices, &self.busy, &self.problem),
                }))
                .await;
        }
    }

    impl Drop for Radio {
        fn drop(&mut self) {
            self.stop_discovery();
        }
    }

    fn listed(
        devices: &HashMap<Address, Speaker>,
        busy: &Option<String>,
        problem: &Option<(String, String)>,
    ) -> Vec<Speaker> {
        let mut speakers: Vec<Speaker> = devices.values().cloned().collect();
        if let Some(address) = busy {
            if let Some(speaker) = speakers
                .iter_mut()
                .find(|speaker| &speaker.address == address)
            {
                speaker.busy = true;
                speaker.problem = None;
            } else {
                speakers.push(Speaker {
                    address: address.clone(),
                    name: address.clone(),
                    paired: false,
                    connected: false,
                    busy: true,
                    problem: None,
                });
            }
        } else if let Some((address, message)) = problem {
            if let Some(speaker) = speakers
                .iter_mut()
                .find(|speaker| &speaker.address == address)
            {
                speaker.problem = Some(message.clone());
            }
        }
        speakers.sort_by(|one, other| {
            one.name
                .to_ascii_lowercase()
                .cmp(&other.name.to_ascii_lowercase())
                .then(one.address.cmp(&other.address))
        });
        speakers
    }

    async fn read_speaker(adapter: &Adapter, address: Address) -> Option<Speaker> {
        let device = adapter.device(address).ok()?;
        let name = match device.alias().await {
            Ok(name) if !name.is_empty() => name,
            _ => device
                .name()
                .await
                .ok()
                .flatten()
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| address.to_string()),
        };
        let heard = Heard {
            class: device.class().await.ok().flatten(),
            icon: device.icon().await.ok().flatten(),
            uuids: device
                .uuids()
                .await
                .ok()
                .flatten()
                .map(|ids| ids.into_iter().map(|id| id.to_string()).collect())
                .unwrap_or_default(),
            paired: device.is_paired().await.unwrap_or(false),
            connected: device.is_connected().await.unwrap_or(false),
            present: matches!(device.rssi().await, Ok(Some(_))),
        };
        if !show_speaker(&heard) {
            return None;
        }
        Some(Speaker {
            address: address.to_string(),
            name,
            paired: heard.paired,
            connected: heard.connected,
            busy: false,
            problem: None,
        })
    }

    async fn connect_device(adapter: &Adapter, address: &str) -> Result<String, String> {
        let parsed: Address = address
            .parse()
            .map_err(|_| "Could not connect".to_string())?;
        let device = adapter
            .device(parsed)
            .map_err(|_| "Could not connect".to_string())?;
        if !device.is_paired().await.unwrap_or(false) {
            device.pair().await.map_err(|error| {
                eprintln!("Bluetooth pair: {error}");
                "Could not pair".to_string()
            })?;
        }
        if let Err(error) = device.set_trusted(true).await {
            eprintln!("Bluetooth trust: {error}");
        }
        if !device.is_connected().await.unwrap_or(false) {
            device.connect().await.map_err(|error| {
                eprintln!("Bluetooth connect: {error}");
                "Could not connect".to_string()
            })?;
        }
        wait_for_output(address).await?;
        let name = device
            .alias()
            .await
            .ok()
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| address.to_string());
        Ok(name)
    }

    async fn wait_for_output(address: &str) -> Result<(), String> {
        let mut last = "The speaker is not an audio output yet.".to_string();
        for _ in 0..OUTPUT_TRIES {
            let address = address.to_string();
            match tokio::task::spawn_blocking(move || set_default_output(&address)).await {
                Ok(Ok(())) => return Ok(()),
                Ok(Err(error)) => last = error,
                Err(error) => last = error.to_string(),
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
        Err(last)
    }

    fn set_default_output(address: &str) -> Result<(), String> {
        if let Some(listing) = command_output("wpctl", &["status"]) {
            if let Some(id) = sink_for_address(&listing, address) {
                if id.chars().all(|ch| ch.is_ascii_digit()) {
                    return command_status("wpctl", &["set-default", &id]);
                }
            }
        }
        if let Some(listing) = command_output("pactl", &["list", "short", "sinks"]) {
            if let Some(name) = sink_for_address(&listing, address) {
                return command_status("pactl", &["set-default-sink", &name]);
            }
        }
        Err("The speaker is not an audio output yet.".to_string())
    }

    fn command_output(program: &str, args: &[&str]) -> Option<String> {
        let output = Command::new(program).args(args).output().ok()?;
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
    }

    fn command_status(program: &str, args: &[&str]) -> Result<(), String> {
        let output = Command::new(program)
            .args(args)
            .output()
            .map_err(|error| error.to_string())?;
        if output.status.success() {
            return Ok(());
        }
        let message = String::from_utf8_lossy(&output.stderr);
        let message = message.trim();
        if message.is_empty() {
            Err(format!("{program} could not set the audio output"))
        } else {
            Err(message.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{due_reconnect, show_speaker, sink_for_address, Heard, Speaker, RECONNECT_AFTER};

    fn speaker(busy: bool, problem: Option<&str>, connected: bool, paired: bool) -> Speaker {
        Speaker {
            address: "AA:BB:CC:DD:EE:FF".to_string(),
            name: "Booth".to_string(),
            paired,
            connected,
            busy,
            problem: problem.map(str::to_string),
        }
    }

    fn heard(class: Option<u32>, icon: Option<&str>, uuids: &[&str]) -> Heard {
        Heard {
            class,
            icon: icon.map(str::to_string),
            uuids: uuids.iter().map(|uuid| (*uuid).to_string()).collect(),
            paired: false,
            connected: false,
            present: true,
        }
    }

    #[test]
    fn the_row_says_what_the_speaker_is_doing() {
        assert_eq!(
            speaker(true, Some("Could not connect"), false, false).detail(),
            "Connecting"
        );
        assert_eq!(
            speaker(false, Some("Could not connect"), true, true).detail(),
            "Could not connect"
        );
        assert_eq!(speaker(false, None, true, true).detail(), "Connected");
        assert_eq!(speaker(false, None, false, true).detail(), "Paired");
        assert_eq!(speaker(false, None, false, false).detail(), "Available");
    }

    #[test]
    fn only_audio_devices_that_can_be_reached_are_listed() {
        let headphones = 0x240418;
        assert!(show_speaker(&heard(Some(headphones), None, &[])));
        assert!(show_speaker(&heard(None, Some("audio-card"), &[])));
        assert!(show_speaker(&heard(
            None,
            None,
            &["0000110B-0000-1000-8000-00805F9B34FB"]
        )));

        let phone = 0x5a020c;
        assert!(!show_speaker(&heard(Some(phone), Some("phone"), &[])));

        let mut stale = heard(Some(headphones), None, &[]);
        stale.present = false;
        assert!(!show_speaker(&stale));
        stale.paired = true;
        assert!(show_speaker(&stale));
    }

    #[test]
    fn the_sink_line_for_the_speaker_is_the_one_that_names_it() {
        let pactl = "\
46\talsa_output.pci.analog-stereo\tPipeWire\ts16le 2ch 48000Hz\n\
63\tbluez_output.AA_BB_CC_DD_EE_FF.a2dp-sink\tPipeWire\ts16le 2ch 48000Hz\n";
        assert_eq!(
            sink_for_address(pactl, "aa:bb:cc:dd:ee:ff").as_deref(),
            Some("bluez_output.AA_BB_CC_DD_EE_FF.a2dp-sink")
        );

        let wpctl = "\
 ├─ Sinks:\n\
 │  *   46. Built-in Audio Analog Stereo\n\
 │      63. bluez_output.AA_BB_CC_DD_EE_FF.1\n";
        assert_eq!(
            sink_for_address(wpctl, "AA:BB:CC:DD:EE:FF").as_deref(),
            Some("63")
        );
        assert_eq!(sink_for_address(wpctl, "11:22:33:44:55:66"), None);
        assert_eq!(sink_for_address(pactl, "not-an-address"), None);
    }

    #[test]
    fn a_remembered_speaker_is_tried_again_until_it_connects() {
        use std::time::Duration;

        assert!(!due_reconnect(true, false, Duration::from_secs(10)));
        assert!(!due_reconnect(false, true, Duration::from_secs(10)));
        assert!(!due_reconnect(
            false,
            false,
            RECONNECT_AFTER - Duration::from_millis(1)
        ));
        assert!(due_reconnect(false, false, RECONNECT_AFTER));
    }
}
