//! `aplan usage` — Claude token consumption over a rolling window.
//!
//! The same figures the HUD's Neural budget panel draws, from the same resolver.
//! It exists for two reasons: the cockpit is driven from the keyboard as much as
//! from the screen, and this is the shortest way to check that the indexing job is
//! actually running without opening the overlay.
//!
//! Everything is computed server-side. This module transports and renders.

use crate::client::Client;
use crate::output::{print_json, ExitCode};
use crate::queries::{neural_budget as usage_query, NeuralBudget as NeuralBudgetQuery};

/// "1.86M" above a million, "412k" above a thousand, the raw integer below —
/// the same scale the HUD panel prints, so the two never disagree at a glance.
fn tokens(count: i64) -> String {
    if count >= 1_000_000 {
        format!("{:.2}M", count as f64 / 1_000_000.0)
    } else if count >= 1_000 {
        format!("{}k", (count as f64 / 1_000.0).round() as i64)
    } else {
        count.to_string()
    }
}

/// A ten-cell bar, tallest day full. Empty days stay visible as a floor cell:
/// a gap would read as a shorter history rather than a quiet day.
fn sparkline(per_day: &[i64]) -> String {
    const BLOCKS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let peak = per_day.iter().copied().max().unwrap_or(0);
    if peak == 0 {
        return BLOCKS[0].to_string().repeat(per_day.len());
    }
    per_day
        .iter()
        .map(|value| {
            let scaled = (*value as f64 / peak as f64 * (BLOCKS.len() - 1) as f64).round() as usize;
            BLOCKS[scaled.min(BLOCKS.len() - 1)]
        })
        .collect()
}

/// The last path segment, for the project line.
fn short_path(path: &str) -> &str {
    let trimmed = path.trim_end_matches('/');
    match trimmed.rfind('/') {
        Some(at) => &trimmed[at + 1..],
        None => trimmed,
    }
}

/// `aplan usage [--window-hours N] [--sparkline-days N]`
pub fn usage(api_url: &str, json: bool, window_hours: i64, sparkline_days: i64) -> ExitCode {
    let client = Client::new(api_url.to_string());
    let vars = usage_query::Variables {
        window_hours,
        sparkline_days,
    };

    match client.run::<NeuralBudgetQuery>(vars) {
        Ok(r) => {
            if json {
                if let Err(e) = print_json(&r.raw) {
                    eprintln!("error writing output: {e}");
                    return ExitCode::Generic;
                }
                return ExitCode::Success;
            }

            let b = &r.data.neural_budget;

            if b.declared_ceiling > 0 {
                println!(
                    "consommé {} / {} ({} %) sur {} h",
                    tokens(b.consumed_tokens),
                    tokens(b.declared_ceiling),
                    (b.consumed_ratio * 100.0).round() as i64,
                    b.window_hours
                );
            } else {
                // No ceiling, no percentage. A ratio against a denominator nobody
                // has set would read as "plenty left" whatever the burn.
                println!(
                    "consommé {} sur {} h — pas de plafond déclaré",
                    tokens(b.consumed_tokens),
                    b.window_hours
                );
                println!(
                    "  à calibrer : aplan config set aplan.claude.declared_ceiling_tokens <n>"
                );
            }

            // Beside the total, never inside it: on a real corpus cache reads run
            // 36x everything else, and folding them in would turn this into a
            // cache-hit rate rather than a burn.
            println!("cache lu {} (hors total)", tokens(b.cache_read_tokens));

            if b.consumed_tokens == 0 {
                println!("aucune consommation sur la fenêtre");
                return ExitCode::Success;
            }

            println!("{}  ({} j)", sparkline(&b.per_day), b.per_day.len());

            for model in &b.per_model {
                println!("  {:<28} {}", model.model, tokens(model.tokens));
            }
            if let Some(project) = &b.top_project {
                println!(
                    "  {:<28} {} %",
                    short_path(&project.name),
                    (project.ratio * 100.0).round() as i64
                );
            }

            ExitCode::Success
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::Generic
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scales_tokens_the_way_the_hud_panel_does() {
        assert_eq!(tokens(999), "999");
        assert_eq!(tokens(1_500), "2k");
        assert_eq!(tokens(412_000), "412k");
        assert_eq!(tokens(1_860_000), "1.86M");
    }

    #[test]
    fn a_sparkline_of_nothing_is_a_flat_floor_not_an_empty_string() {
        // Ten quiet days and no history at all must not render the same width.
        assert_eq!(sparkline(&[0, 0, 0]), "▁▁▁");
        assert_eq!(sparkline(&[]), "");
    }

    #[test]
    fn the_tallest_day_fills_the_cell_and_the_others_scale_to_it() {
        // Half of the peak lands on index 3.5 of an eight-block scale, which
        // rounds up — the fifth block, not the fourth.
        assert_eq!(sparkline(&[0, 50, 100]), "▁▅█");
        assert_eq!(sparkline(&[100, 100]), "██");
    }

    #[test]
    fn an_empty_day_between_busy_ones_keeps_its_cell() {
        // A gap would read as a shorter history rather than a day off.
        assert_eq!(sparkline(&[100, 0, 100]).chars().count(), 3);
        assert_eq!(sparkline(&[100, 0, 100]).chars().nth(1), Some('▁'));
    }

    #[test]
    fn a_project_shows_by_its_last_segment() {
        assert_eq!(short_path("/home/mbt/appfactory/aggregated_plan"), "aggregated_plan");
        assert_eq!(short_path("/home/mbt/appfactory/aggregated_plan/"), "aggregated_plan");
        assert_eq!(short_path("aggregated_plan"), "aggregated_plan");
    }
}
