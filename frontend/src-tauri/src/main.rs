// Minimal shell. On Wayland the app_id is derived by GTK from the executable
// name, which `productName = "aplan-hud"` in tauri.conf.json already sets.
// Step 6 verifies that empirically rather than trusting it — if the compositor
// reports something else, the windowrule follows the measurement, not this file.
use std::sync::Mutex;
use std::time::Instant;

use serde::Serialize;
use tauri::Emitter;
use sysinfo::{Networks, System};

/// One sample of local machine telemetry for the HUD's Station block.
/// `#[serde(rename_all = "camelCase")]` bridges Rust's own snake_case field
/// names to the camelCase this codebase uses at every other data boundary
/// (GraphQL payloads, every other block's props) — see
/// `frontend/src/pages/hud/blocks/StationBlock.tsx`, which mirrors this
/// struct exactly.
///
/// CPU/RAM/network never touch GraphQL or SQLite: design doc §5
/// (`docs/plans/2026-08-27-hud-overlay-tauri-design.md`) routes this data
/// through a Tauri IPC command specifically because it is local and
/// ephemeral — there is nothing here worth historising.
#[derive(Serialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
struct StationStats {
    cpu_percent: f32,
    ram_used_bytes: u64,
    ram_total_bytes: u64,
    net_rx_bytes_per_sec: u64,
    net_tx_bytes_per_sec: u64,
}

/// CPU usage and network throughput are both DELTAS, not instantaneous
/// reads: sysinfo computes CPU usage as the average since the previous
/// `refresh_cpu_usage()` call, and `NetworkData::received`/`transmitted`
/// report bytes since the previous `Networks::refresh()` call. A fresh
/// `System`/`Networks` built inside the command on every call would have no
/// previous sample to diff against and would always read zero. Kept in
/// Tauri-managed state instead, so each poll refines against the one
/// before it — the frontend already paces calls at `STATS_POLL_MS` (2s),
/// comfortably above sysinfo's minimum refresh interval.
struct StationState {
    sys: System,
    networks: Networks,
    last_poll: Instant,
}

impl StationState {
    fn new() -> Self {
        let mut sys = System::new_all();
        sys.refresh_cpu_usage();
        Self {
            sys,
            networks: Networks::new_with_refreshed_list(),
            last_poll: Instant::now(),
        }
    }
}

#[tauri::command]
fn station_stats(state: tauri::State<'_, Mutex<StationState>>) -> StationStats {
    // A HUD readout must not go permanently dark over one panic while the
    // lock was held: `std::sync::Mutex` never un-poisons itself, so
    // `.expect()` here would make every future call panic forever, and the
    // frontend's own `.catch(() => {})` would swallow that silently — the
    // Station block frozen on its last values with nothing on screen to say
    // anything broke. Nothing in the critical section below can corrupt
    // `StationState` on panic (it's plain field reads/writes, no invariant
    // spanning multiple fields), so recovering the poisoned data is sound.
    let mut state = state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    state.sys.refresh_cpu_usage();
    state.sys.refresh_memory();
    // `false`: this is a HUD readout, not an inventory — a network
    // interface that disappears mid-session (e.g. Wi-Fi toggled) is left in
    // the map rather than pruned, since dropping it would also drop its
    // running byte count out from under the rx/tx sum below.
    state.networks.refresh(false);

    let elapsed_secs = state.last_poll.elapsed().as_secs_f64().max(0.001);
    let (rx, tx) = state
        .networks
        .iter()
        .fold((0u64, 0u64), |(rx, tx), (_name, data)| {
            (rx + data.received(), tx + data.transmitted())
        });
    state.last_poll = Instant::now();

    StationStats {
        cpu_percent: state.sys.global_cpu_usage(),
        ram_used_bytes: state.sys.used_memory(),
        ram_total_bytes: state.sys.total_memory(),
        net_rx_bytes_per_sec: (rx as f64 / elapsed_secs) as u64,
        net_tx_bytes_per_sec: (tx as f64 / elapsed_secs) as u64,
    }
}

/// The event the frontend listens on to learn whether the overlay is actually
/// on screen. Payload: `true` shown, `false` hidden.
const SURFACE_VISIBILITY: &str = "surface-visibility";

fn main() {
    // NOT SIGUSR1/SIGUSR2. JavaScriptCore already owns SIGUSR1 for garbage
    // collection: installing a handler over it made WebKit say so out loud —
    // "Overriding existing handler for signal 10. Set JSC_SIGNAL_FOR_GC…" —
    // and then killed the HUD on the openings where a collection happened to
    // fall. Real-time signals are claimed by nobody and are queued rather
    // than coalesced.
    //
    // Registered BEFORE Tauri starts, and that ordering is load-bearing: the
    // toggle script can signal within ~50 ms of launch, while the dynamic
    // linker is still pulling in WebKitGTK — far earlier than `setup()` would
    // run. Measured at that delay, an unregistered signal hit the default
    // disposition and the process died. The script also stays silent on the
    // launch path; the two guards overlap on purpose.
    let shown_signal = libc::SIGRTMIN();
    let hidden_signal = libc::SIGRTMIN() + 1;
    let mut signals = signal_hook::iterator::Signals::new([shown_signal, hidden_signal])
        .expect("failed to register the surface-visibility signal handlers");

    tauri::Builder::default()
        .manage(Mutex::new(StationState::new()))
        .invoke_handler(tauri::generate_handler![station_stats])
        .setup(move |app| {
            // Measured on the real compositor: when Hyprland hides the special
            // workspace this window lives on, the webview never notices —
            // `document.visibilityState` stays "visible" for as long as the
            // overlay is off screen. Every visibility gate in the HUD was
            // therefore inert, and the boot sequence played out behind the
            // curtain where nobody could see it.
            //
            // `aplan-hud-toggle` is the only thing that knows, because it is
            // what performs the toggle. It signals us afterwards, and we turn
            // that into an ordinary Tauri event. Done on a thread rather than
            // in a handler: a signal handler may not allocate or take a lock,
            // and emitting an event does both.
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                for signal in signals.forever() {
                    let shown = signal == shown_signal;
                    // A failed emit means the webview is gone, which is not
                    // something this thread can or should fix.
                    if let Err(error) = handle.emit(SURFACE_VISIBILITY, shown) {
                        // Loud on purpose. The mirror-image failure on the
                        // webview side — `listen()` refused by the ACL — was
                        // silent, and cost an hour of looking in the wrong
                        // place while this side reported success.
                        eprintln!("aplan-hud: could not emit {SURFACE_VISIBILITY}: {error}");
                    }
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("failed to start the aplan HUD shell");
}
