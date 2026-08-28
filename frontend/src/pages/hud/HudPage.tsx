import { useEffect, useRef, useState } from 'react';
import { useSurfaceVisibility } from './useSurfaceVisibility';
import { useDominantBlock } from './useDominantBlock';
import { HudNav } from './HudNav';
import { FocusBlock } from './blocks/FocusBlock';
import { PressureBlock } from './blocks/PressureBlock';
import { AgendaBlock } from './blocks/AgendaBlock';
import { NeuralBudgetBlock } from './blocks/NeuralBudgetBlock';
import { AgentsBlock } from './blocks/AgentsBlock';
import { StationBlock } from './blocks/StationBlock';
import { Ticker } from './blocks/Ticker';
import { HUD_VERSION } from './hud-version';
import './hud.css';

const BOOT_LINES = [
  `aplan cockpit v${HUD_VERSION}`,
  'link 127.0.0.1:3001 ......... ok',
  'palette cybernord .......... ok',
  'session bus ................ ok',
] as const;

const BOOT_MS = 1500;

/** The grid proper, split out of `HudPage` so that nothing behind the boot
 *  sequence touches the network: `useDominantBlock` reads the dashboard, and
 *  the six blocks each read their own hooks — all of it mounts when the boot
 *  animation ends, not while it plays. */
function HudGrid() {
  // One arbiter, one glow: every `lit` below is derived from this single
  // value, so no combination of props can light two panels at once.
  const dominant = useDominantBlock();

  return (
    <div data-testid="hud-grid" className="hud">
      <HudNav />
      <FocusBlock lit={dominant === 'focus'} />
      <PressureBlock lit={dominant === 'pressure'} />
      <AgendaBlock lit={dominant === 'agenda'} />
      <NeuralBudgetBlock />
      <AgentsBlock />
      <StationBlock />
      <Ticker />
    </div>
  );
}

export function HudPage() {
  const [booting, setBooting] = useState(true);
  const visible = useSurfaceVisibility();
  const bootDeadline = useRef<number | null>(null);

  // The window is born on a hidden Hyprland special workspace, so a sequence
  // started at mount plays out entirely behind the curtain and is never seen.
  // It starts at first VISIBILITY instead.
  //
  // The deadline is stamped once and never re-stamped: the sequence gets one
  // chance per process. That settles the case the plan left open — hiding the
  // surface mid-boot (SUPER+B 300 ms in) must not buy a second showing, and
  // must not strand the grid behind a boot screen either. On return we serve
  // whatever is left of the original window, which after any real absence is
  // nothing, and the grid appears at once.
  useEffect(() => {
    if (!visible || !booting) return;
    if (bootDeadline.current === null) bootDeadline.current = Date.now() + BOOT_MS;
    const remaining = Math.max(0, bootDeadline.current - Date.now());
    const t = setTimeout(() => setBooting(false), remaining);
    return () => clearTimeout(t);
  }, [visible, booting]);

  return (
    <div
      className="hud-viewport h-screen w-screen bg-transparent font-cn text-cn-fg"
      data-surface-visible={visible}
    >
      {booting ? (
        <pre data-testid="boot-sequence" className="p-8 text-sm text-cn-teal">
          {BOOT_LINES.join('\n')}
        </pre>
      ) : (
        <HudGrid />
      )}
    </div>
  );
}
