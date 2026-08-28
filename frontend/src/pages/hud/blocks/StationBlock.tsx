import { useEffect, useState } from 'react';
import { isTauri, invoke } from '@tauri-apps/api/core';
import { format } from 'date-fns';
import { useSurfaceVisibility } from '../useSurfaceVisibility';

/** The clock only ever shows HH:MM, so a per-second tick is more than
 *  enough headroom — mirrors FocusBlock/AgendaBlock's own tick rate so the
 *  whole HUD reads one convention for "how often does time move". */
const CLOCK_TICK_MS = 1000;

/** How often the Rust side is asked for a fresh CPU/RAM/network sample.
 *  Cheap enough over local IPC that this is a UI-freshness choice, not a
 *  performance one. */
const STATS_POLL_MS = 2000;

/** Shape returned by the `station_stats` Tauri command (see
 *  src-tauri/src/main.rs). camelCase here — the Rust struct is
 *  `#[serde(rename_all = "camelCase")]` precisely so the wire format matches
 *  every other prop in this codebase, even though Rust itself writes the
 *  field names in its own snake_case. */
export interface StationStats {
  readonly cpuPercent: number;
  readonly ramUsedBytes: number;
  readonly ramTotalBytes: number;
  readonly netRxBytesPerSec: number;
  readonly netTxBytesPerSec: number;
}

function formatClock(d: Date): string {
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

/** `Friday 28 August` — no year, matching the mockup's own "jeudi 27 août"
 *  (a "today" readout has no use for one). English, like every other
 *  visible string in the HUD's secondary blocks. */
function formatStationDate(d: Date): string {
  return format(d, 'EEEE d MMMM');
}

const BYTES_PER_GB = 1_000_000_000;
const BYTES_PER_MB = 1_000_000;

/** `11.4 G` — decimal (SI) gigabytes, not binary GiB: matches how most
 *  desktop system monitors label "G" for memory, and keeps this block's
 *  math simple and consistent with the network rate below. */
function formatRam(usedBytes: number): string {
  return `${(usedBytes / BYTES_PER_GB).toFixed(1)} G`;
}

/** Combined rx+tx as a single "how busy is the network" figure — the
 *  design doc doesn't specify direction-by-direction display, and a single
 *  number is what the mockup's `.sys` grid has room for. Decimal MB/s. */
function formatNetRate(bytesPerSec: number): string {
  return `${(bytesPerSec / BYTES_PER_MB).toFixed(1)} M/s`;
}

export function StationBlock() {
  const surfaceVisible = useSurfaceVisibility();
  const [now, setNow] = useState(() => new Date());
  const [stats, setStats] = useState<StationStats | null>(null);

  // The wall clock: gated on surface visibility per the HUD's own rule
  // (mirrors FocusBlock's chronometer) — a ticking clock behind a hidden
  // window is exactly the cost `useSurfaceVisibility` exists to avoid.
  useEffect(() => {
    if (!surfaceVisible) return;
    setNow(new Date());
    const id = setInterval(() => setNow(new Date()), CLOCK_TICK_MS);
    return () => clearInterval(id);
  }, [surfaceVisible]);

  // CPU/RAM/network: outside Tauri the `station_stats` command does not
  // exist at all, so this effect must not even attempt the call, let alone
  // poll it — `stats` then stays permanently null, which is exactly what
  // tells the render below to fall back to clock-and-date only, per the
  // task brief ("sans case vide ni valeur inventée"). Polling also stops
  // the moment the surface is hidden, same rule as the clock above.
  useEffect(() => {
    if (!isTauri() || !surfaceVisible) return;
    let cancelled = false;
    const poll = () => {
      invoke<StationStats>('station_stats')
        .then((sample) => {
          if (!cancelled) setStats(sample);
        })
        .catch(() => {
          // A failed sample is not a reason to blank out the last good one
          // or to fabricate one — skip this tick, try again on the next.
        });
    };
    poll();
    const id = setInterval(poll, STATS_POLL_MS);
    return () => {
      cancelled = true;
      clearInterval(id);
    };
  }, [surfaceVisible]);

  return (
    <div className="hud-panel hud-station" data-testid="station-block">
      <div className="hud-label">▌ Station</div>
      <div className="hud-station__clock" data-testid="station-clock">
        {formatClock(now)}
      </div>
      <div className="hud-station__date">{formatStationDate(now)}</div>

      {stats ? (
        <div className="hud-station__sys" data-testid="station-sys">
          <div>
            <span>CPU</span>
            <b>{Math.round(stats.cpuPercent)}%</b>
          </div>
          <div>
            <span>RAM</span>
            <b>{formatRam(stats.ramUsedBytes)}</b>
          </div>
          <div>
            <span>Net</span>
            <b>{formatNetRate(stats.netRxBytesPerSec + stats.netTxBytesPerSec)}</b>
          </div>
        </div>
      ) : (
        !isTauri() && (
          <div className="hud-station__unavailable" data-testid="station-unavailable">
            System telemetry unavailable outside the desktop app
          </div>
        )
      )}
    </div>
  );
}
