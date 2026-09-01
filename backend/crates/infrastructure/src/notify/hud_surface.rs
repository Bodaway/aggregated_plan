use async_trait::async_trait;
use tokio::process::Command;

use application::errors::AppError;
use application::services::SurfaceController;

/// The toggle script, resolved on the `PATH` so a packaged install and a checkout both
/// work without configuration.
pub const DEFAULT_PROGRAM: &str = "aplan-hud-toggle";

/// Overrides the program above — an absolute path during development, when the script
/// is not installed anywhere the service's `PATH` reaches.
pub const PROGRAM_ENV: &str = "APLAN_HUD_TOGGLE";

/// What the surface is being asked to do. The two subcommands are idempotent, so the
/// caller never has to know what is currently on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceAction {
    Show,
    Hide,
}

impl SurfaceAction {
    fn subcommand(self) -> &'static str {
        match self {
            SurfaceAction::Show => "show",
            SurfaceAction::Hide => "hide",
        }
    }
}

/// Pick the program to run and its argv.
///
/// Split out as a pure function on purpose, exactly like `notify_send::command_args`:
/// the override is the only decision here, and the spawn below is a three-line shell.
/// It takes the override as an argument rather than reading the environment itself,
/// because a test that had to set a process-wide variable would race every other test
/// in the binary.
pub fn command_line(action: SurfaceAction, program_override: Option<&str>) -> (String, Vec<String>) {
    // A variable set to the empty string is a variable nobody meant to set — exporting
    // `APLAN_HUD_TOGGLE=` in a unit file must not turn every break into a spawn of "".
    let program = program_override
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .unwrap_or(DEFAULT_PROGRAM)
        .to_string();
    (program, vec![action.subcommand().to_string()])
}

/// Drives the Tauri HUD overlay through `aplan-hud-toggle show` / `... hide`.
///
/// A script rather than a compositor call from here: knowing which workspace the
/// overlay lives on, and which Hyprland instance is current, is the script's job and
/// it already does it for the keyboard shortcut.
pub struct HudToggleSurface;

impl HudToggleSurface {
    pub fn new() -> Self {
        Self
    }

    async fn run(&self, action: SurfaceAction) -> Result<(), AppError> {
        let (program, args) = command_line(action, std::env::var(PROGRAM_ENV).ok().as_deref());
        let output = Command::new(&program)
            .args(&args)
            .output()
            .await
            .map_err(|e| AppError::Internal(format!("{program} failed to run: {e}")))?;
        if !output.status.success() {
            // Returned rather than panicked: the caller logs it and serves the break
            // anyway. A missing overlay is not a reason to cancel a pause.
            return Err(AppError::Internal(format!(
                "{program} {} exited with {}: {}",
                action.subcommand(),
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        Ok(())
    }
}

impl Default for HudToggleSurface {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SurfaceController for HudToggleSurface {
    async fn show(&self) -> Result<(), AppError> {
        self.run(SurfaceAction::Show).await
    }

    async fn hide(&self) -> Result<(), AppError> {
        self.run(SurfaceAction::Hide).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_action_maps_to_its_subcommand() {
        assert_eq!(command_line(SurfaceAction::Show, None).1, vec!["show"]);
        assert_eq!(command_line(SurfaceAction::Hide, None).1, vec!["hide"]);
    }

    /// Unset means "find it on the PATH", which is how the installed script is reached.
    #[test]
    fn the_program_defaults_to_the_bare_name() {
        assert_eq!(command_line(SurfaceAction::Show, None).0, DEFAULT_PROGRAM);
    }

    #[test]
    fn the_override_replaces_the_program_and_nothing_else() {
        let (program, args) = command_line(SurfaceAction::Hide, Some("/opt/aplan/hud-toggle"));
        assert_eq!(program, "/opt/aplan/hud-toggle");
        assert_eq!(args, vec!["hide"]);
    }

    /// `APLAN_HUD_TOGGLE=` in a unit file is a variable nobody meant to set. Honouring
    /// it would spawn the empty string and take the overlay down for good.
    #[test]
    fn a_blank_override_falls_back_to_the_default() {
        assert_eq!(command_line(SurfaceAction::Show, Some("")).0, DEFAULT_PROGRAM);
        assert_eq!(command_line(SurfaceAction::Show, Some("   ")).0, DEFAULT_PROGRAM);
    }
}
