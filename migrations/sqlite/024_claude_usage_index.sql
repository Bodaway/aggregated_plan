-- The Claude usage index: what the Neural budget block of the HUD reads.
--
-- Source: `~/.claude/projects/**/*.jsonl`, the Claude Code transcripts. Measured on
-- 2026-09-11: 661 files, 629 MB, spanning two months, growing ~45% a fortnight.
--
-- ONE ROW PER API REQUEST, NOT PER LINE. This is the whole reason `request_id` is the
-- primary key rather than a rowid with an index. A single API call emits one
-- `assistant` line per content block — a thinking block, a text block, a tool_use
-- block — and *every one of them repeats the same `usage` object verbatim*. Measured:
-- 56 151 lines carry usage, for 28 675 distinct requests. Summing lines inflates the
-- burn by a factor of 1.89, which would show a gauge at 68% as 36%.
--
-- Making the request id the key buys idempotence for free: re-reading a file writes
-- the same rows, so the incremental cursor below is an optimisation and never a
-- correctness mechanism. A lost offset, a rewritten file or a wiped index cost one
-- rescan (~2s for the whole corpus), never a wrong total.
--
-- No `user_id`, the one deliberate exception to that project-wide rule: these rows
-- describe the machine's own Claude consumption, not a cockpit user's data. A
-- `user_id` here would be a constant column nobody would ever filter on.
CREATE TABLE IF NOT EXISTS claude_usage_requests (
    request_id                  TEXT PRIMARY KEY,
    -- ISO-8601 UTC, straight from the line's `timestamp`. Lexicographic order is
    -- chronological order, which is all the rolling windows need.
    occurred_at                 TEXT NOT NULL,
    model                       TEXT NOT NULL,
    session_id                  TEXT NOT NULL,
    -- The line's own `cwd`, normalised: a git worktree under `.claude/worktrees/`
    -- folds back onto its parent checkout. Taken from `cwd` and never decoded from
    -- the containing folder name, which replaces every separator with a dash and so
    -- cannot tell `aggregated_plan` from `aggregated-plan`.
    project_path                TEXT NOT NULL,
    -- Subagent turns. Nearly half the corpus (26 645 of 56 151 lines) and real
    -- tokens: counted, but kept separable because "how much do my subagents cost"
    -- is the question this column exists to answer.
    is_sidechain                INTEGER NOT NULL DEFAULT 0,
    input_tokens                INTEGER NOT NULL DEFAULT 0,
    output_tokens               INTEGER NOT NULL DEFAULT 0,
    cache_creation_input_tokens INTEGER NOT NULL DEFAULT 0,
    -- Stored, never added to the consumed total. It measures 4.99 billion tokens
    -- against 143 million for the other three combined — 36x — so including it
    -- would turn the gauge into a cache-hit meter.
    cache_read_input_tokens     INTEGER NOT NULL DEFAULT 0,
    -- A *part of* output_tokens (it lives in `output_tokens_details`), never a
    -- separate cost. Verified: it did not exceed output_tokens on a single one of
    -- 28 637 requests. Stored for information; adding it would inflate production
    -- by 38%.
    thinking_tokens             INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_claude_usage_occurred_at
    ON claude_usage_requests (occurred_at);
CREATE INDEX IF NOT EXISTS idx_claude_usage_project
    ON claude_usage_requests (project_path, occurred_at);

-- Where the last pass stopped in each transcript, so a tick reads the new tail
-- instead of 629 MB. A cache, not a ledger: see the idempotence note above.
--
-- `size_bytes` is what makes a rewritten file safe. A transcript that has SHRUNK
-- below its recorded offset was compacted or replaced, and its byte offsets no longer
-- point at line boundaries — the reader starts over from zero rather than resuming
-- into the middle of a line.
CREATE TABLE IF NOT EXISTS claude_usage_files (
    path            TEXT PRIMARY KEY,
    size_bytes      INTEGER NOT NULL,
    mtime           TEXT NOT NULL,
    offset_bytes    INTEGER NOT NULL,
    last_indexed_at TEXT NOT NULL
);
