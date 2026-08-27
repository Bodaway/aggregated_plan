// Minimal shell. On Wayland the app_id is derived by GTK from the executable
// name, which `productName = "aplan-hud"` in tauri.conf.json already sets.
// Step 6 verifies that empirically rather than trusting it — if the compositor
// reports something else, the windowrule follows the measurement, not this file.
fn main() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("failed to start the aplan HUD shell");
}
