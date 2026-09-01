use async_trait::async_trait;

use crate::errors::AppError;

/// Raises and lowers the break surface — the full-screen countdown the user looks at
/// instead of the screen they were working on.
///
/// A trait rather than a direct call for the same reason `Notifier` is one: the
/// application layer decides *that* the break is on screen, and never spawns the
/// process that puts it there. It also lets the whole session run headless in tests
/// without a compositor.
#[async_trait]
pub trait SurfaceController: Send + Sync {
    async fn show(&self) -> Result<(), AppError>;
    async fn hide(&self) -> Result<(), AppError>;
}

/// Shows nothing, hides nothing, always succeeds.
///
/// Selected at wiring time when there is no graphical session. It reports success
/// rather than failure on purpose: the break still runs, the deadline is still the
/// backend's, and a break served without a visual is a degraded break, not a failed
/// tick.
pub struct NullSurface;

#[async_trait]
impl SurfaceController for NullSurface {
    async fn show(&self) -> Result<(), AppError> {
        Ok(())
    }

    async fn hide(&self) -> Result<(), AppError> {
        Ok(())
    }
}
