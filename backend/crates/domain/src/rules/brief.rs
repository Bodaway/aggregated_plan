//! Composition and rendering of `aplan brief` — what a Claude session is handed
//! at startup.
//!
//! Everything here is pure. The caller fetches rows and prints strings; this
//! module decides what goes in, in which order, and how it is cut down. The line
//! ceiling is enforced *here* rather than at the call site because this output
//! enters the model's context on every session, forever: an unbounded rendering
//! is a permanent token leak, not a cosmetic problem.
//!
//! The brief **adds to** the session's followed-task list, it never replaces it
//! (§7.2 of the design): the deadline set and the followed set are different
//! sets, and the task list feeds the start-up task picker.

use chrono::{DateTime, Datelike, NaiveDate, Utc};

use crate::types::{Memory, MemoryId, MemoryKind, ProjectId, Task, TaskStatus, TrackingState};

/// Hard ceiling on the rendered brief. Pinned by a test: the brief is injected in
/// every session, so this is a budget, not a guideline.
pub const BRIEF_MAX_LINES: usize = 40;

/// Hard ceiling on one rendered line, in characters. Without it a single 500-char
/// memory title would blow the token budget while still counting as "one line".
pub const BRIEF_MAX_LINE_CHARS: usize = 140;

/// Beyond this many days without a consolidation run, the brief warns. Below it,
/// nothing is said — the line exists to surface a silent breakage (§6.2).
pub const CONSOLIDATION_STALE_AFTER_DAYS: i64 = 3;

/// Per-section entry ceilings. Chosen so the full rendering provably fits in
/// [`BRIEF_MAX_LINES`]; the pathological-input test is what keeps that true.
pub const MAX_DEADLINE_ENTRIES: usize = 6;
pub const MAX_COMMITMENT_ENTRIES: usize = 8;
pub const MAX_DECISION_ENTRIES: usize = 6;
/// Working rules are few and very stable: a low ceiling suffices, and it keeps
/// the section under the ~50 tokens that justify it being the last one cut.
pub const MAX_PREFERENCE_ENTRIES: usize = 4;

/// Shortest memory reference rendered (`m:7c1`). Long enough to be typed back,
/// short enough to keep a line readable.
pub const MEMORY_REF_MIN_CHARS: usize = 3;
/// A hyphenated UUID in full.
pub const MEMORY_REF_MAX_CHARS: usize = 36;

/// Which brief is being produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BriefVariant {
    /// The SessionStart injection (§7.2): everything, with drill-down hints.
    Session,
    /// The 08:30 desktop notification (§7.3): today's deadlines, open
    /// commitments, candidates to triage. No decisions, no drill-down hints —
    /// a notification is read, not queried.
    Morning,
}

impl BriefVariant {
    pub fn as_str(&self) -> &'static str {
        match self {
            BriefVariant::Session => "session",
            BriefVariant::Morning => "morning",
        }
    }
}

/// Everything the brief needs, already fetched. The slices are read-only: this
/// module never mutates and never performs I/O.
#[derive(Debug, Clone, Copy)]
pub struct BriefInput<'a> {
    pub variant: BriefVariant,
    /// Local "today", used for the deadline countdowns.
    pub today: NaiveDate,
    /// Used for the consolidation age only.
    pub now: DateTime<Utc>,
    /// Candidate tasks. Filtering (open, not dismissed, not a test fixture) is
    /// done here, so passing the whole task list is correct if wasteful.
    pub tasks: &'a [Task],
    /// Candidate memories, expected to be the recallable ones. Non-recallable
    /// rows are dropped here anyway — the brief must never show a superseded fact.
    pub memories: &'a [Memory],
    /// The project in focus, if any. Narrows the decisions section.
    pub current_project: Option<ProjectId>,
    /// Candidates waiting in the validation queue.
    pub pending_count: usize,
    /// When the consolidation job last ran. `None` = never (lot 5 does not exist
    /// yet, so this is the normal case for now — it must not look like a crash).
    pub last_consolidation: Option<DateTime<Utc>>,
}

/// A task deadline, reduced to what the brief shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeadlineEntry {
    pub title: String,
    /// Days from today to the deadline. Negative = overdue.
    pub days_until: i64,
}

/// A memory the brief points at. `reference` is the short form (`m:7c1`) that
/// `aplan recall` accepts — without it the brief would be a dead end.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryEntry {
    pub id: MemoryId,
    pub reference: String,
    pub title: String,
    pub stakeholders: Vec<String>,
    pub occurred_on: NaiveDate,
}

/// A section plus the count it was cut down from, so truncation is never silent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BriefSection<T> {
    pub entries: Vec<T>,
    /// How many qualified, before the section cap.
    pub total: usize,
}

impl<T> BriefSection<T> {
    fn empty() -> Self {
        Self {
            entries: Vec::new(),
            total: 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Qualifying items not shown.
    pub fn hidden(&self) -> usize {
        self.total.saturating_sub(self.entries.len())
    }
}

/// Age of the last consolidation run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsolidationAge {
    /// No run ever recorded — a fresh install, or a job that has never fired.
    NeverRun,
    Ran { days_ago: i64 },
}

impl ConsolidationAge {
    /// Whether the brief warns about it. Never-run counts as stale: an
    /// consolidation that has never fired is exactly the breakage to surface.
    pub fn is_stale(&self) -> bool {
        match self {
            ConsolidationAge::NeverRun => true,
            ConsolidationAge::Ran { days_ago } => *days_ago > CONSOLIDATION_STALE_AFTER_DAYS,
        }
    }
}

/// The composed brief: a value object, renderable and inspectable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Brief {
    pub variant: BriefVariant,
    pub date: NaiveDate,
    /// Working rules. First rendered, last cut.
    pub preferences: BriefSection<MemoryEntry>,
    pub deadlines: BriefSection<DeadlineEntry>,
    pub commitments: BriefSection<MemoryEntry>,
    pub decisions: BriefSection<MemoryEntry>,
    /// Whether the decisions section was narrowed to a project.
    pub decisions_scoped_to_project: bool,
    pub pending_count: usize,
    pub consolidation: ConsolidationAge,
}

impl Brief {
    /// True when nothing at all is worth saying.
    pub fn is_silent(&self) -> bool {
        self.preferences.is_empty()
            && self.deadlines.is_empty()
            && self.commitments.is_empty()
            && self.decisions.is_empty()
            && self.pending_count == 0
            && !self.consolidation.is_stale()
    }
}

// ─── Selection rules ─────────────────────────────────────────────────────────

/// Whether a title is one of the test fixtures polluting the store (~550 tasks,
/// mostly duplicated test rows: `Test uppercase kind` ×16, `Test recurring enum`
/// ×16, `test` ×2).
///
/// Deliberately narrow: only a *leading* `test` / `fixture` word counts, so a
/// French task ("Tests de charge", "Recette Cartier") is kept. A task genuinely
/// named "Test de charge" is the accepted false positive.
pub fn is_test_fixture_title(title: &str) -> bool {
    let first = title
        .split_whitespace()
        .next()
        .unwrap_or("")
        .trim_matches(|c: char| !c.is_alphanumeric())
        .to_lowercase();
    first == "test" || first == "fixture"
}

/// Case- and whitespace-insensitive title key, used to collapse duplicates.
fn title_key(title: &str) -> String {
    title
        .split_whitespace()
        .map(|w| w.to_lowercase())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Whether a task can appear in the deadline section.
///
/// Dismissed tasks are excluded — the user said no. Inbox tasks are kept: an
/// untriaged task with a deadline tomorrow is precisely what must be surfaced.
pub fn is_deadline_candidate(task: &Task) -> bool {
    task.deadline.is_some()
        && !matches!(task.status, TaskStatus::Done | TaskStatus::Cancelled)
        && task.tracking_state != TrackingState::Dismissed
        && !is_test_fixture_title(&task.title)
}

/// Deadlines to show, **nearest to today first**, deduplicated by title.
///
/// Ordering by proximity rather than by date is deliberate. The real store holds
/// tasks overdue by 250+ days — abandoned Jira rows — and a plain "most overdue
/// first" sort fills the whole section with archaeology while burying what falls
/// due this week. At equal distance the overdue side wins: it is already late.
///
/// Deduplication matters twice over: recurring tasks materialise one row per
/// occurrence (`SAFT: rouler le script des heures JIRA.` ×17), and the fixtures
/// are literal duplicates. The nearest of a duplicate group survives.
///
/// `Morning` keeps only today's and overdue deadlines (§7.3, "échéances du jour").
pub fn select_deadlines(
    tasks: &[Task],
    today: NaiveDate,
    variant: BriefVariant,
    cap: usize,
) -> BriefSection<DeadlineEntry> {
    let mut kept: Vec<DeadlineEntry> = Vec::new();
    let mut seen: Vec<String> = Vec::new();

    let mut candidates: Vec<(NaiveDate, &Task)> = tasks
        .iter()
        .filter(|t| is_deadline_candidate(t))
        .filter_map(|t| t.deadline.map(|d| (d, t)))
        .filter(|(deadline, _)| match variant {
            BriefVariant::Session => true,
            BriefVariant::Morning => *deadline <= today,
        })
        .collect();
    // Nearest to today first, overdue ahead at equal distance; title breaks the
    // remaining ties so the order is stable across runs.
    candidates.sort_by_key(|(deadline, task)| {
        let days = (*deadline - today).num_days();
        (days.abs(), days, task.title.clone())
    });

    for (deadline, task) in candidates {
        let key = title_key(&task.title);
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        kept.push(DeadlineEntry {
            title: task.title.trim().to_string(),
            days_until: (deadline - today).num_days(),
        });
    }

    let total = kept.len();
    kept.truncate(cap);
    BriefSection {
        entries: kept,
        total,
    }
}

/// Recallable memories of one kind, in the order the brief wants them.
fn memories_of_kind(memories: &[Memory], kind: MemoryKind) -> Vec<&Memory> {
    memories
        .iter()
        // The hard filter of §7.1 again, locally: a brief must never carry a
        // superseded or unvalidated fact, whatever the caller passed in.
        .filter(|m| m.is_recallable() && m.kind == kind)
        .collect()
}

/// Working rules, **newest first**: a preference restated recently is the one
/// that currently holds. Rendered before everything else and cut last — the
/// section is both the most useful and the cheapest (three short lines).
pub fn select_preferences(memories: &[Memory], cap: usize) -> BriefSection<MemoryEntry> {
    let mut rows = memories_of_kind(memories, MemoryKind::Preference);
    rows.sort_by(|a, b| b.occurred_at.cmp(&a.occurred_at).then_with(|| a.id.cmp(&b.id)));
    section_from(rows, cap)
}

/// Open commitments, **oldest first**: a promise made three months ago and still
/// open is the one that has been forgotten, and the whole point of the section is
/// to un-forget it. (Decisions are ordered the other way — see
/// [`select_decisions`].)
pub fn select_commitments(memories: &[Memory], cap: usize) -> BriefSection<MemoryEntry> {
    let mut rows = memories_of_kind(memories, MemoryKind::Commitment);
    rows.sort_by(|a, b| a.occurred_at.cmp(&b.occurred_at).then_with(|| a.id.cmp(&b.id)));
    section_from(rows, cap)
}

/// Active decisions, **newest first**: here the question is "where does the
/// project currently stand", and the latest arbitration is the answer.
///
/// When a project is in focus, only its decisions are kept. With no project in
/// focus the section falls back to every active decision rather than going empty
/// — an empty section teaches Claude nothing.
pub fn select_decisions(
    memories: &[Memory],
    current_project: Option<ProjectId>,
    cap: usize,
) -> BriefSection<MemoryEntry> {
    let mut rows: Vec<&Memory> = memories_of_kind(memories, MemoryKind::Decision)
        .into_iter()
        .filter(|m| match current_project {
            None => true,
            Some(project) => m.project_id == Some(project),
        })
        .collect();
    rows.sort_by(|a, b| b.occurred_at.cmp(&a.occurred_at).then_with(|| a.id.cmp(&b.id)));
    section_from(rows, cap)
}

fn section_from(rows: Vec<&Memory>, cap: usize) -> BriefSection<MemoryEntry> {
    let total = rows.len();
    let entries: Vec<MemoryEntry> = rows
        .into_iter()
        .take(cap)
        .map(|m| MemoryEntry {
            id: m.id,
            // Filled in by `compose_brief`, which alone knows every id it will
            // render and can therefore pick a width that keeps them distinct.
            reference: String::new(),
            title: m.title.trim().to_string(),
            stakeholders: m.stakeholders.clone(),
            occurred_on: m.occurred_at.date_naive(),
        })
        .collect();
    BriefSection { entries, total }
}

// ─── Memory references ───────────────────────────────────────────────────────

/// Short reference to a memory: `m:7c1`, the drill-down handle of §7.2.
///
/// Built from the **hyphenated** id so that a prefix of the reference is also a
/// prefix of the stored value — `aplan recall m:7c1` resolves by prefix, and the
/// un-hyphenated form would stop matching past the eighth character.
pub fn memory_reference(id: MemoryId, chars: usize) -> String {
    let text = id.to_string();
    let width = chars.clamp(MEMORY_REF_MIN_CHARS, MEMORY_REF_MAX_CHARS);
    format!("m:{}", text.chars().take(width).collect::<String>())
}

/// The shortest reference width that keeps every id in `ids` distinct.
///
/// A brief that renders two identical references is a brief whose drill-down is
/// broken, so the width grows until it cannot happen — within this brief. A
/// collision with a memory *not* in the brief is still possible and is handled
/// where it is detected: the lookup reports an ambiguity instead of guessing.
pub fn memory_reference_width(ids: &[MemoryId]) -> usize {
    let mut unique: Vec<MemoryId> = Vec::new();
    for id in ids {
        if !unique.contains(id) {
            unique.push(*id);
        }
    }
    for width in MEMORY_REF_MIN_CHARS..MEMORY_REF_MAX_CHARS {
        let mut refs: Vec<String> = unique.iter().map(|id| memory_reference(*id, width)).collect();
        refs.sort();
        let before = refs.len();
        refs.dedup();
        if refs.len() == before {
            return width;
        }
    }
    MEMORY_REF_MAX_CHARS
}

/// Turn what a reader typed back — `[7c1]`, `7c1`, or a full UUID — into a
/// lowercase id prefix. `None` when there is nothing usable, so a bad token never
/// reaches a `LIKE` pattern.
///
/// Not memory-specific: every aggregate whose ids a human retypes needs the same
/// parsing, and the worklog reattribution resolves entry references through it.
/// One definition means one answer to "is this token usable" — two would drift, and
/// the drift would show up as a reference that resolves for one verb and not for
/// another.
pub fn parse_id_reference(token: &str) -> Option<String> {
    let body = token
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim()
        .to_lowercase();
    if body.is_empty() || body.len() > MEMORY_REF_MAX_CHARS {
        return None;
    }
    if !body.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
        return None;
    }
    if !body.chars().any(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    Some(body)
}

/// Turn what a reader typed back — `[m:7c1]`, `m:7c1`, `7c1`, or a full UUID —
/// into a lowercase id prefix. `None` when there is nothing usable, so a bad
/// token never reaches a `LIKE` pattern.
pub fn parse_memory_reference(token: &str) -> Option<String> {
    let trimmed = token.trim().trim_start_matches('[').trim_end_matches(']');
    let body = trimmed
        .strip_prefix("m:")
        .or_else(|| trimmed.strip_prefix("M:"))
        .unwrap_or(trimmed);
    parse_id_reference(body)
}

// ─── Composition ─────────────────────────────────────────────────────────────

/// Compose the brief, deciding both content and how much of it survives the line
/// budget.
///
/// Truncation runs from the least useful section to the most useful: decisions
/// are cut first, then commitments, then deadlines, and preferences last (R55).
/// Preferences and deadlines are each sized from a fixed per-section constant
/// rather than from the remaining budget, so neither is itself budget-capped —
/// what makes preferences the last one sacrificed is that their cost, like the
/// deadlines', is reserved *before* the commitment and decision caps are
/// computed, so it is never invisible to what those two get to keep. Every cut
/// is reported through [`BriefSection::hidden`], so the rendering can say it
/// happened.
pub fn compose_brief(input: &BriefInput) -> Brief {
    let consolidation = match input.last_consolidation {
        None => ConsolidationAge::NeverRun,
        Some(last) => ConsolidationAge::Ran {
            days_ago: (input.now - last).num_days().max(0),
        },
    };

    let morning = input.variant == BriefVariant::Morning;
    // Reserved before the deadlines, so a pathological deadline list can never
    // squeeze the working rules out: R55's sacrifice order ends with them.
    let preferences = select_preferences(input.memories, MAX_PREFERENCE_ENTRIES);
    let deadlines = select_deadlines(
        input.tasks,
        input.today,
        input.variant,
        MAX_DEADLINE_ENTRIES,
    );

    // Line budget: the header, plus the lines the rendering always spends.
    let mut budget = BRIEF_MAX_LINES.saturating_sub(1);
    if !preferences.is_empty() {
        budget = budget.saturating_sub(1 + preferences.entries.len());
    }
    if !deadlines.is_empty() {
        // A label plus one line per deadline.
        budget = budget.saturating_sub(1 + deadlines.entries.len());
    }
    if input.pending_count > 0 {
        budget = budget.saturating_sub(1);
    }
    if consolidation.is_stale() {
        budget = budget.saturating_sub(1);
    }
    if !morning {
        // The drill-down footer.
        budget = budget.saturating_sub(1);
    }

    // Commitments first — a promise outranks a piece of context.
    let commitment_cap = MAX_COMMITMENT_ENTRIES.min(budget.saturating_sub(1));
    let commitments = select_commitments(input.memories, commitment_cap);
    if !commitments.is_empty() {
        budget = budget.saturating_sub(1 + commitments.entries.len());
    }

    let decisions = if morning {
        // §7.3: the morning notification carries no decisions.
        BriefSection::empty()
    } else {
        let cap = MAX_DECISION_ENTRIES.min(budget.saturating_sub(1));
        select_decisions(input.memories, input.current_project, cap)
    };

    let mut brief = Brief {
        variant: input.variant,
        date: input.today,
        preferences,
        deadlines,
        commitments,
        decisions,
        decisions_scoped_to_project: !morning && input.current_project.is_some(),
        pending_count: input.pending_count,
        consolidation,
    };

    // References last: the width depends on every id that ended up in the brief.
    let ids: Vec<MemoryId> = brief
        .preferences
        .entries
        .iter()
        .chain(brief.commitments.entries.iter())
        .chain(brief.decisions.entries.iter())
        .map(|e| e.id)
        .collect();
    let width = memory_reference_width(&ids);
    for entry in brief
        .preferences
        .entries
        .iter_mut()
        .chain(brief.commitments.entries.iter_mut())
        .chain(brief.decisions.entries.iter_mut())
    {
        entry.reference = memory_reference(entry.id, width);
    }

    brief
}

// ─── Rendering ───────────────────────────────────────────────────────────────

const WEEKDAYS_FR: [&str; 7] = [
    "lundi",
    "mardi",
    "mercredi",
    "jeudi",
    "vendredi",
    "samedi",
    "dimanche",
];

const MONTHS_FR: [&str; 12] = [
    "janvier",
    "février",
    "mars",
    "avril",
    "mai",
    "juin",
    "juillet",
    "août",
    "septembre",
    "octobre",
    "novembre",
    "décembre",
];

/// `lundi 3 août`. Written here rather than pulled from a locale crate: three
/// arrays beat a dependency, and the domain has none beyond chrono.
pub fn format_date_fr(date: NaiveDate) -> String {
    let weekday = WEEKDAYS_FR[date.weekday().num_days_from_monday() as usize];
    let month = MONTHS_FR[(date.month0()) as usize];
    format!("{weekday} {} {month}", date.day())
}

/// `12/06` — enough to place a memory in time without spending a line on it.
fn format_day_month(date: NaiveDate) -> String {
    format!("{:02}/{:02}", date.day(), date.month())
}

/// `J-5` / `aujourd'hui` / `retard 3j`. `J+3` was rejected as ambiguous: it reads
/// as "in three days" as easily as "three days late".
fn format_countdown(days_until: i64) -> String {
    match days_until {
        0 => "aujourd'hui".to_string(),
        d if d < 0 => format!("retard {}j", -d),
        d => format!("J-{d}"),
    }
}

/// Clamp one line to [`BRIEF_MAX_LINE_CHARS`], marking the cut.
fn clamp_line(line: String) -> String {
    if line.chars().count() <= BRIEF_MAX_LINE_CHARS {
        return line;
    }
    let kept: String = line.chars().take(BRIEF_MAX_LINE_CHARS - 1).collect();
    format!("{kept}…")
}

/// `Engagements ouverts (2) :`, or `Engagements ouverts (12, 8 affichés) :` when
/// the cap bit. The count *is* the truncation notice, so it costs no extra line.
fn count_label(label: &str, total: usize, shown: usize) -> String {
    if total == shown {
        format!("{label} ({total}) :")
    } else {
        format!("{label} ({total}, {shown} affichés) :")
    }
}

fn render_memory_section(lines: &mut Vec<String>, label: &str, section: &BriefSection<MemoryEntry>) {
    if section.is_empty() {
        return;
    }
    lines.push(count_label(label, section.total, section.entries.len()));
    for entry in &section.entries {
        let who = if entry.stakeholders.is_empty() {
            String::new()
        } else {
            format!("{} — ", entry.stakeholders.join(", "))
        };
        lines.push(format!(
            "  [{}] {}{} ({})",
            entry.reference,
            who,
            entry.title,
            format_day_month(entry.occurred_on)
        ));
    }
}

/// Render the brief, one string per line, **never more than
/// [`BRIEF_MAX_LINES`]**.
pub fn render_brief(brief: &Brief) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();

    let title = match brief.variant {
        BriefVariant::Session => "## Brief",
        BriefVariant::Morning => "## Brief du matin",
    };
    lines.push(format!("{title} — {}", format_date_fr(brief.date)));

    if brief.is_silent() {
        // Said here rather than appended at the end: a bare header reads like a
        // failure, and "nothing to report" is a real answer.
        lines.push("Rien à signaler.".to_string());
        return lines;
    }

    if !brief.deadlines.is_empty() {
        // One line per deadline. The design's sample put them on a single line,
        // but real titles run to a hundred characters: joined, two survived the
        // line clamp and the "… +6" marker was itself clipped off — silent
        // truncation, which is the one thing this rendering must not do.
        lines.push(count_label("Échéances", brief.deadlines.total, brief.deadlines.entries.len()));
        for entry in &brief.deadlines.entries {
            lines.push(format!(
                "  {} — {}",
                format_countdown(entry.days_until),
                entry.title
            ));
        }
    }

    render_memory_section(&mut lines, "Engagements ouverts", &brief.commitments);
    let decisions_label = if brief.decisions_scoped_to_project {
        "Décisions actives — projet courant"
    } else {
        "Décisions actives"
    };
    render_memory_section(&mut lines, decisions_label, &brief.decisions);

    if brief.pending_count > 0 {
        lines.push(format!(
            "À trier : {} candidat{} mémoire → `aplan inbox`",
            brief.pending_count,
            if brief.pending_count == 1 { "" } else { "s" }
        ));
    }

    match brief.consolidation {
        ConsolidationAge::NeverRun => {
            lines.push("⚠ Dernière consolidation : jamais exécutée".to_string());
        }
        ConsolidationAge::Ran { days_ago } if days_ago > CONSOLIDATION_STALE_AFTER_DAYS => {
            lines.push(format!(
                "⚠ Dernière consolidation : il y a {days_ago} jours"
            ));
        }
        ConsolidationAge::Ran { .. } => {}
    }

    if brief.variant == BriefVariant::Session {
        // The footer names a reference that actually exists, so the hint is
        // copy-pasteable rather than decorative.
        let sample = brief
            .decisions
            .entries
            .first()
            .or_else(|| brief.commitments.entries.first())
            .map(|e| e.reference.clone());
        match sample {
            Some(reference) => lines.push(format!(
                "Détail : `aplan recall {reference}` · Recherche : `aplan recall --q \"…\"`"
            )),
            None => lines.push("Recherche : `aplan recall --q \"…\"`".to_string()),
        }
    }

    let mut lines: Vec<String> = lines.into_iter().map(clamp_line).collect();
    enforce_line_cap(&mut lines);
    lines
}

/// Last-resort guard on the ceiling. `compose_brief` sizes its sections to fit,
/// so this should never fire — but "should never" is not "cannot", and a silent
/// overrun is a token leak nobody would notice.
fn enforce_line_cap(lines: &mut Vec<String>) {
    if lines.len() <= BRIEF_MAX_LINES {
        return;
    }
    let dropped = lines.len() - (BRIEF_MAX_LINES - 1);
    lines.truncate(BRIEF_MAX_LINES - 1);
    lines.push(format!(
        "… brief tronqué : {dropped} lignes masquées (plafond {BRIEF_MAX_LINES})"
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{
        ImpactLevel, MemorySource, MemoryStatus, Source, TaskId, UrgencyLevel, UserId,
    };
    use chrono::TimeZone;
    use uuid::Uuid;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 8, 3).expect("valid date")
    }

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 3, 8, 30, 0).unwrap()
    }

    fn task(title: &str, deadline: Option<NaiveDate>) -> Task {
        Task {
            id: TaskId::new_v4(),
            user_id: UserId::nil(),
            title: title.to_string(),
            description: None,
            notes: None,
            source: Source::Personal,
            source_id: None,
            jira_status: None,
            status: TaskStatus::Todo,
            project_id: None,
            assignee: None,
            delegated_to: None,
            deadline,
            planned_start: None,
            planned_end: None,
            estimated_hours: None,
            urgency: UrgencyLevel::Low,
            urgency_manual: false,
            impact: ImpactLevel::Low,
            tags: vec![],
            tracking_state: TrackingState::Followed,
            jira_remaining_seconds: None,
            jira_original_estimate_seconds: None,
            jira_time_spent_seconds: None,
            remaining_hours_override: None,
            estimated_hours_override: None,
            recurrence_id: None,
            occurrence_date: None,
            gryzzly_task_id: None,
            gryzzly_project_id: None,
            created_at: now(),
            updated_at: now(),
        }
    }

    fn in_days(n: i64) -> Option<NaiveDate> {
        Some(today() + chrono::Duration::days(n))
    }

    fn memory(kind: MemoryKind, title: &str, days_ago: i64) -> Memory {
        Memory {
            id: Uuid::new_v4(),
            user_id: UserId::nil(),
            kind,
            title: title.to_string(),
            body: None,
            occurred_at: now() - chrono::Duration::days(days_ago),
            recorded_at: now(),
            invalidated_at: None,
            superseded_by: None,
            proposed_supersedes: None,
            source: MemorySource::ClaudeSession,
            source_ref: None,
            status: MemoryStatus::Active,
            project_id: None,
            task_id: None,
            stakeholders: vec![],
        }
    }

    fn input<'a>(tasks: &'a [Task], memories: &'a [Memory]) -> BriefInput<'a> {
        BriefInput {
            variant: BriefVariant::Session,
            today: today(),
            now: now(),
            tasks,
            memories,
            current_project: None,
            pending_count: 0,
            last_consolidation: Some(now()),
        }
    }

    // ─── The ceiling ─────────────────────────────────────────────────────

    /// The whole reason this module exists. If this test fails, the brief has
    /// started leaking tokens into every future session.
    #[test]
    fn a_pathological_brief_still_fits_the_line_ceiling() {
        let long = "x".repeat(500);
        let tasks: Vec<Task> = (0..120)
            .map(|i| task(&format!("{long} tâche {i}"), in_days(i % 40 - 10)))
            .collect();
        let mut memories: Vec<Memory> = Vec::new();
        for i in 0..120 {
            let mut commitment = memory(MemoryKind::Commitment, &format!("{long} promesse {i}"), i);
            commitment.stakeholders = vec![long.clone(), "Pierre".into()];
            memories.push(commitment);
            memories.push(memory(MemoryKind::Decision, &format!("{long} choix {i}"), i));
        }

        let brief = compose_brief(&BriefInput {
            pending_count: 999,
            last_consolidation: None,
            ..input(&tasks, &memories)
        });
        let lines = render_brief(&brief);

        assert!(
            lines.len() <= BRIEF_MAX_LINES,
            "rendered {} lines, ceiling is {BRIEF_MAX_LINES}:\n{}",
            lines.len(),
            lines.join("\n")
        );
        for line in &lines {
            assert!(
                line.chars().count() <= BRIEF_MAX_LINE_CHARS,
                "line of {} chars exceeds {BRIEF_MAX_LINE_CHARS}: {line}",
                line.chars().count()
            );
        }
    }

    /// The guard behind the guard: even handed a brief that ignores every cap,
/// the rendering stays inside the ceiling and says it cut something.
    #[test]
    fn the_ceiling_is_enforced_even_on_an_oversized_brief() {
        let entries: Vec<MemoryEntry> = (0..200)
            .map(|i| MemoryEntry {
                id: Uuid::new_v4(),
                reference: format!("m:{i:03x}"),
                title: format!("promesse {i}"),
                stakeholders: vec![],
                occurred_on: today(),
            })
            .collect();
        let brief = Brief {
            variant: BriefVariant::Session,
            date: today(),
            preferences: BriefSection::empty(),
            deadlines: BriefSection::empty(),
            commitments: BriefSection {
                total: entries.len(),
                entries,
            },
            decisions: BriefSection::empty(),
            decisions_scoped_to_project: false,
            pending_count: 0,
            consolidation: ConsolidationAge::Ran { days_ago: 0 },
        };

        let lines = render_brief(&brief);
        assert_eq!(lines.len(), BRIEF_MAX_LINES);
        assert!(
            lines.last().expect("a last line").contains("tronqué"),
            "truncation must be visible, got: {:?}",
            lines.last()
        );
    }

    #[test]
    fn truncation_is_visible_in_every_section() {
        let tasks: Vec<Task> = (0..20)
            .map(|i| task(&format!("tâche {i}"), in_days(i)))
            .collect();
        let memories: Vec<Memory> = (0..20)
            .flat_map(|i| {
                [
                    memory(MemoryKind::Commitment, &format!("promesse {i}"), i),
                    memory(MemoryKind::Decision, &format!("choix {i}"), i),
                ]
            })
            .collect();
        let brief = compose_brief(&input(&tasks, &memories));
        let text = render_brief(&brief).join("\n");

        assert!(text.contains("Échéances (20, 6 affichés) :"), "{text}");
        assert!(
            text.contains("Engagements ouverts (20, 8 affichés) :"),
            "{text}"
        );
        assert!(text.contains("Décisions actives (20, 6 affichés) :"), "{text}");
    }

    // ─── Memory references ───────────────────────────────────────────────

    #[test]
    fn a_reference_is_the_short_hyphenated_prefix() {
        let id = Uuid::parse_str("7c1e4b2a-0000-0000-0000-000000000000").expect("valid uuid");
        assert_eq!(memory_reference(id, 3), "m:7c1");
        // A prefix of the reference is a prefix of the stored id, past the
        // eighth character too — that is why the hyphenated form is used.
        assert_eq!(memory_reference(id, 10), "m:7c1e4b2a-0");
        assert!(id.to_string().starts_with("7c1e4b2a-0"));
    }

    #[test]
    fn a_reference_is_never_shorter_than_the_minimum() {
        let id = Uuid::new_v4();
        assert_eq!(
            memory_reference(id, 0).len(),
            2 + MEMORY_REF_MIN_CHARS,
            "a 1-char reference would be unusable"
        );
    }

    #[test]
    fn the_reference_width_grows_until_the_ids_are_distinguishable() {
        let a = Uuid::parse_str("7c1e0000-0000-0000-0000-000000000000").expect("valid");
        let b = Uuid::parse_str("7c1f0000-0000-0000-0000-000000000000").expect("valid");
        assert_eq!(memory_reference_width(&[a]), MEMORY_REF_MIN_CHARS);
        assert_eq!(
            memory_reference_width(&[a, b]),
            4,
            "3 chars collide, so the brief widens rather than render two `m:7c1`"
        );
        assert_eq!(
            memory_reference_width(&[a, a]),
            MEMORY_REF_MIN_CHARS,
            "the same id twice is not a collision"
        );
    }

    #[test]
    fn every_rendered_reference_is_unique() {
        let a = Uuid::parse_str("7c1e0000-0000-0000-0000-000000000000").expect("valid");
        let b = Uuid::parse_str("7c1f0000-0000-0000-0000-000000000000").expect("valid");
        let mut m1 = memory(MemoryKind::Decision, "choix A", 1);
        m1.id = a;
        let mut m2 = memory(MemoryKind::Decision, "choix B", 2);
        m2.id = b;
        let brief = compose_brief(&input(&[], &[m1, m2]));
        let refs: Vec<&str> = brief
            .decisions
            .entries
            .iter()
            .map(|e| e.reference.as_str())
            .collect();
        assert_eq!(refs, vec!["m:7c1e", "m:7c1f"]);
    }

    #[test]
    fn a_reference_survives_a_round_trip_through_the_reader() {
        let id = Uuid::parse_str("7c1e4b2a-0000-0000-0000-000000000000").expect("valid uuid");
        let rendered = memory_reference(id, 3);
        assert_eq!(parse_memory_reference(&rendered), Some("7c1".to_string()));
        assert_eq!(
            parse_memory_reference(&format!("[{rendered}]")),
            Some("7c1".to_string()),
            "the rendering shows brackets, so the reader may type them"
        );
        assert_eq!(parse_memory_reference("7C1"), Some("7c1".to_string()));
        assert_eq!(
            parse_memory_reference(&id.to_string()),
            Some(id.to_string()),
            "a full uuid must still pass"
        );
    }

    #[test]
    fn a_reference_that_could_poison_a_pattern_is_refused() {
        for token in ["", "  ", "m:", "[]", "%", "7c1%", "m:zz", "'; DROP", "-"] {
            assert_eq!(
                parse_memory_reference(token),
                None,
                "token {token:?} must be refused"
            );
        }
    }

    // ─── Deadlines ───────────────────────────────────────────────────────

    /// Nearest to today first, overdue ahead at equal distance. A task overdue by
    /// eight months must not push next week's deadline out of the section — the
    /// real store is full of the former.
    #[test]
    fn deadlines_are_ordered_by_proximity_not_by_date() {
        let tasks = vec![
            task("abandonnée", in_days(-256)),
            task("loin", in_days(42)),
            task("hier", in_days(-1)),
            task("demain", in_days(1)),
        ];
        let section = select_deadlines(&tasks, today(), BriefVariant::Session, 6);
        let titles: Vec<&str> = section.entries.iter().map(|e| e.title.as_str()).collect();
        assert_eq!(titles, vec!["hier", "demain", "loin", "abandonnée"]);
        assert_eq!(section.entries[0].days_until, -1);
        assert_eq!(section.entries[3].days_until, -256);
    }

    #[test]
    fn the_test_fixtures_polluting_the_store_are_filtered_out() {
        let tasks = vec![
            task("Test uppercase kind", in_days(1)),
            task("Test recurring enum", in_days(1)),
            task("test", in_days(1)),
            task("Pernod Ricard — Azure Assessment Report", in_days(5)),
        ];
        let section = select_deadlines(&tasks, today(), BriefVariant::Session, 6);
        assert_eq!(section.total, 1);
        assert_eq!(section.entries[0].title, "Pernod Ricard — Azure Assessment Report");
    }

    #[test]
    fn a_french_title_starting_with_tests_is_not_taken_for_a_fixture() {
        assert!(is_test_fixture_title("Test uppercase kind"));
        assert!(is_test_fixture_title("  test  "));
        assert!(is_test_fixture_title("Test: recurring enum"));
        assert!(!is_test_fixture_title("Tests de charge Cartier"));
        assert!(!is_test_fixture_title("Recette et tests"));
        assert!(!is_test_fixture_title(""));
    }

    /// A recurring task materialises one row per occurrence — 17 of them in the
    /// real store. The brief shows the soonest, once.
    #[test]
    fn duplicate_titles_collapse_to_the_soonest() {
        let tasks = vec![
            task("SAFT: rouler le script des heures", in_days(9)),
            task("saft: Rouler   le script des heures", in_days(2)),
            task("SAFT: rouler le script des heures", in_days(16)),
        ];
        let section = select_deadlines(&tasks, today(), BriefVariant::Session, 6);
        assert_eq!(section.total, 1);
        assert_eq!(section.entries[0].days_until, 2);
    }

    #[test]
    fn closed_and_dismissed_tasks_never_reach_the_brief() {
        let mut done = task("terminée", in_days(1));
        done.status = TaskStatus::Done;
        let mut cancelled = task("annulée", in_days(1));
        cancelled.status = TaskStatus::Cancelled;
        let mut dismissed = task("écartée", in_days(1));
        dismissed.tracking_state = TrackingState::Dismissed;
        let mut untriaged = task("non triée", in_days(1));
        untriaged.tracking_state = TrackingState::Inbox;

        let section = select_deadlines(
            &[done, cancelled, dismissed, untriaged, task("sans échéance", None)],
            today(),
            BriefVariant::Session,
            6,
        );
        let titles: Vec<&str> = section.entries.iter().map(|e| e.title.as_str()).collect();
        assert_eq!(
            titles,
            vec!["non triée"],
            "an untriaged task with a deadline is exactly what must surface"
        );
    }

    #[test]
    fn the_morning_variant_keeps_only_today_and_overdue() {
        let tasks = vec![
            task("hier", in_days(-2)),
            task("aujourd'hui", in_days(0)),
            task("demain", in_days(1)),
        ];
        let section = select_deadlines(&tasks, today(), BriefVariant::Morning, 6);
        let titles: Vec<&str> = section.entries.iter().map(|e| e.title.as_str()).collect();
        assert_eq!(titles, vec!["aujourd'hui", "hier"], "nearest first, as everywhere");
    }

    // ─── Memory sections ─────────────────────────────────────────────────

    #[test]
    fn only_recallable_memories_reach_the_brief() {
        let mut superseded = memory(MemoryKind::Decision, "périmée", 10);
        superseded.invalidated_at = Some(now());
        superseded.superseded_by = Some(Uuid::new_v4());
        let mut pending = memory(MemoryKind::Decision, "pas validée", 5);
        pending.status = MemoryStatus::Pending;
        let live = memory(MemoryKind::Decision, "vraie", 1);

        let section = select_decisions(&[superseded, pending, live], None, 6);
        assert_eq!(section.total, 1);
        assert_eq!(section.entries[0].title, "vraie");
    }

    #[test]
    fn commitments_are_oldest_first_and_carry_their_stakeholders() {
        let mut recent = memory(MemoryKind::Commitment, "récente", 2);
        recent.stakeholders = vec!["Sophie".into()];
        let mut old = memory(MemoryKind::Commitment, "ancienne", 90);
        old.stakeholders = vec!["Pierre".into(), "Marc".into()];

        let section = select_commitments(&[recent, old], 8);
        assert_eq!(section.entries[0].title, "ancienne");
        assert_eq!(
            section.entries[0].stakeholders,
            vec!["Pierre".to_string(), "Marc".to_string()]
        );
        assert_eq!(section.entries[1].title, "récente");
    }

    #[test]
    fn decisions_are_newest_first() {
        let section = select_decisions(
            &[
                memory(MemoryKind::Decision, "ancienne", 90),
                memory(MemoryKind::Decision, "récente", 2),
            ],
            None,
            6,
        );
        assert_eq!(section.entries[0].title, "récente");
    }

    #[test]
    fn decisions_narrow_to_the_project_in_focus() {
        let project = Uuid::new_v4();
        let mut mine = memory(MemoryKind::Decision, "du projet courant", 1);
        mine.project_id = Some(project);
        let other = memory(MemoryKind::Decision, "d'un autre projet", 1);

        let scoped = select_decisions(&[mine.clone(), other.clone()], Some(project), 6);
        assert_eq!(scoped.total, 1);
        assert_eq!(scoped.entries[0].title, "du projet courant");

        let unscoped = select_decisions(&[mine, other], None, 6);
        assert_eq!(
            unscoped.total, 2,
            "with no project in focus, an empty section would teach nothing"
        );
    }

    #[test]
    fn preferences_are_selected_newest_first() {
        let memories = vec![
            memory(MemoryKind::Preference, "ancienne règle", 90),
            memory(MemoryKind::Preference, "règle du jour", 1),
            memory(MemoryKind::Fact, "un fait, pas une règle", 2),
        ];

        let section = select_preferences(&memories, 10);

        assert_eq!(section.total, 2, "seules les préférences comptent");
        assert_eq!(section.entries[0].title, "règle du jour");
        assert_eq!(section.entries[1].title, "ancienne règle");
    }

    #[test]
    fn preferences_report_what_the_cap_hid() {
        let memories = vec![
            memory(MemoryKind::Preference, "une", 1),
            memory(MemoryKind::Preference, "deux", 2),
            memory(MemoryKind::Preference, "trois", 3),
        ];

        let section = select_preferences(&memories, 2);

        assert_eq!(section.entries.len(), 2);
        assert_eq!(section.total, 3);
        assert_eq!(section.hidden(), 1, "la troncature n'est jamais silencieuse");
    }

    #[test]
    fn composed_brief_carries_preferences_with_references() {
        let memories = vec![memory(MemoryKind::Preference, "une idée par slide", 3)];
        let input = BriefInput {
            variant: BriefVariant::Session,
            today: today(),
            now: now(),
            tasks: &[],
            memories: &memories,
            current_project: None,
            pending_count: 0,
            last_consolidation: Some(now()),
        };

        let brief = compose_brief(&input);

        assert_eq!(brief.preferences.entries.len(), 1);
        assert!(
            brief.preferences.entries[0].reference.starts_with("m:"),
            "sans référence courte la ligne est un cul-de-sac (R56)"
        );
    }

    #[test]
    fn a_brief_holding_only_a_preference_is_not_silent() {
        let memories = vec![memory(MemoryKind::Preference, "une idée par slide", 3)];
        let input = BriefInput {
            variant: BriefVariant::Session,
            today: today(),
            now: now(),
            tasks: &[],
            memories: &memories,
            current_project: None,
            pending_count: 0,
            last_consolidation: Some(now()),
        };

        assert!(!compose_brief(&input).is_silent());
    }

    #[test]
    fn facts_and_preferences_are_not_brief_material() {
        let memories = vec![
            memory(MemoryKind::Fact, "le crate mcp ne compile pas", 1),
            memory(MemoryKind::Preference, "notes atomiques", 1),
        ];
        let brief = compose_brief(&input(&[], &memories));
        assert!(brief.commitments.is_empty());
        assert!(brief.decisions.is_empty());
    }

    // ─── Consolidation staleness ─────────────────────────────────────────

    #[test]
    fn a_missing_consolidation_timestamp_reads_as_never_run() {
        let brief = compose_brief(&BriefInput {
            last_consolidation: None,
            ..input(&[], &[])
        });
        assert_eq!(brief.consolidation, ConsolidationAge::NeverRun);
        assert!(brief.consolidation.is_stale());
        let text = render_brief(&brief).join("\n");
        assert!(text.contains("⚠ Dernière consolidation : jamais exécutée"), "{text}");
    }

    #[test]
    fn a_fresh_consolidation_is_not_mentioned_at_all() {
        for days in 0..=CONSOLIDATION_STALE_AFTER_DAYS {
            let brief = compose_brief(&BriefInput {
                last_consolidation: Some(now() - chrono::Duration::days(days)),
                ..input(&[], &[])
            });
            assert!(!brief.consolidation.is_stale(), "{days} days must stay quiet");
            let text = render_brief(&brief).join("\n");
            assert!(
                !text.contains("consolidation"),
                "{days} days must not print a warning: {text}"
            );
        }
    }

    #[test]
    fn a_stale_consolidation_is_reported_with_its_age() {
        let brief = compose_brief(&BriefInput {
            last_consolidation: Some(now() - chrono::Duration::days(19)),
            ..input(&[], &[])
        });
        assert_eq!(brief.consolidation, ConsolidationAge::Ran { days_ago: 19 });
        let text = render_brief(&brief).join("\n");
        assert!(text.contains("il y a 19 jours"), "{text}");
    }

    // ─── Rendering ───────────────────────────────────────────────────────

    #[test]
    fn the_rendering_follows_the_shape_of_the_design() {
        let project = Uuid::new_v4();
        let mut commitment = memory(
            MemoryKind::Commitment,
            "Répondre à Pierre sur l'archi AI Microsoft",
            52,
        );
        commitment.stakeholders = vec!["Pierre".into()];
        let mut decision = memory(
            MemoryKind::Decision,
            "Wave 0 limitée au périmètre AI Microsoft",
            52,
        );
        decision.project_id = Some(project);

        let tasks = vec![
            task("Cartier certificat", in_days(42)),
            task("Pernod assessment", in_days(5)),
        ];
        let brief = compose_brief(&BriefInput {
            current_project: Some(project),
            pending_count: 4,
            last_consolidation: Some(now() - chrono::Duration::days(19)),
            ..input(&tasks, &[commitment, decision])
        });
        let lines = render_brief(&brief);

        assert_eq!(
            lines,
            vec![
                "## Brief — lundi 3 août".to_string(),
                "Échéances (2) :".to_string(),
                "  J-5 — Pernod assessment".to_string(),
                "  J-42 — Cartier certificat".to_string(),
                "Engagements ouverts (1) :".to_string(),
                format!(
                    "  [{}] Pierre — Répondre à Pierre sur l'archi AI Microsoft (12/06)",
                    brief.commitments.entries[0].reference
                ),
                "Décisions actives — projet courant (1) :".to_string(),
                format!(
                    "  [{}] Wave 0 limitée au périmètre AI Microsoft (12/06)",
                    brief.decisions.entries[0].reference
                ),
                "À trier : 4 candidats mémoire → `aplan inbox`".to_string(),
                "⚠ Dernière consolidation : il y a 19 jours".to_string(),
                format!(
                    "Détail : `aplan recall {}` · Recherche : `aplan recall --q \"…\"`",
                    brief.decisions.entries[0].reference
                ),
            ]
        );
    }

    /// Without a reference in the rendering the brief is a dead end: nothing can
    /// be expanded, and just-in-time retrieval never happens.
    #[test]
    fn every_memory_line_carries_a_reference_that_recall_accepts() {
        let commitment = memory(MemoryKind::Commitment, "une promesse", 3);
        let decision = memory(MemoryKind::Decision, "un arbitrage", 3);
        let ids = [commitment.id, decision.id];
        let brief = compose_brief(&input(&[], &[commitment, decision]));
        let lines = render_brief(&brief);

        for (entry, id) in brief
            .commitments
            .entries
            .iter()
            .chain(brief.decisions.entries.iter())
            .zip(ids.iter())
        {
            let prefix = parse_memory_reference(&entry.reference).expect("a parseable reference");
            assert!(
                id.to_string().starts_with(&prefix),
                "reference {} does not point at {id}",
                entry.reference
            );
            assert!(
                lines.iter().any(|l| l.contains(&entry.reference)),
                "reference {} is missing from the rendering",
                entry.reference
            );
        }
    }

    #[test]
    fn the_morning_variant_drops_the_decisions_and_the_hints() {
        let commitment = memory(MemoryKind::Commitment, "une promesse", 3);
        let decision = memory(MemoryKind::Decision, "un arbitrage", 3);
        let brief = compose_brief(&BriefInput {
            variant: BriefVariant::Morning,
            pending_count: 2,
            ..input(&[], &[commitment, decision])
        });
        let text = render_brief(&brief).join("\n");

        assert!(text.starts_with("## Brief du matin — lundi 3 août"), "{text}");
        assert!(!text.contains("Décisions actives"), "{text}");
        assert!(!text.contains("aplan recall"), "{text}");
        assert!(text.contains("Engagements ouverts (1) :"), "{text}");
        assert!(text.contains("À trier : 2 candidats mémoire"), "{text}");
    }

    #[test]
    fn an_empty_store_says_so_instead_of_printing_a_bare_header() {
        let brief = compose_brief(&input(&[], &[]));
        assert!(brief.is_silent());
        assert_eq!(
            render_brief(&brief),
            vec![
                "## Brief — lundi 3 août".to_string(),
                "Rien à signaler.".to_string()
            ],
            "nothing to report costs two lines, hints included"
        );
    }

    #[test]
    fn the_footer_hint_only_names_a_reference_when_there_is_one() {
        let lines = render_brief(&compose_brief(&input(
            &[task("Cartier certificat", in_days(3))],
            &[],
        )));
        let footer = lines.last().expect("a footer");
        assert_eq!(footer, "Recherche : `aplan recall --q \"…\"`");
    }

    #[test]
    fn a_countdown_never_reads_ambiguously() {
        assert_eq!(format_countdown(5), "J-5");
        assert_eq!(format_countdown(0), "aujourd'hui");
        assert_eq!(format_countdown(-3), "retard 3j");
    }

    #[test]
    fn the_date_is_written_in_french() {
        assert_eq!(
            format_date_fr(NaiveDate::from_ymd_opt(2026, 8, 3).unwrap()),
            "lundi 3 août"
        );
        assert_eq!(
            format_date_fr(NaiveDate::from_ymd_opt(2026, 12, 25).unwrap()),
            "vendredi 25 décembre"
        );
    }
}
