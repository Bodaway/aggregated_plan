// Minimal shell. On Wayland the app_id is derived by GTK from the executable
// name, which `productName = "aplan-hud"` in tauri.conf.json already sets.
// Step 6 verifies that empirically rather than trusting it — if the compositor
// reports something else, the windowrule follows the measurement, not this file.
use std::sync::Mutex;
use std::time::Instant;

use serde::Serialize;
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
    let mut state = state.lock().expect("station state mutex poisoned");

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

fn main() {
    tauri::Builder::default()
        .manage(Mutex::new(StationState::new()))
        .invoke_handler(tauri::generate_handler![station_stats])
        .run(tauri::generate_context!())
        .expect("failed to start the aplan HUD shell");
}
