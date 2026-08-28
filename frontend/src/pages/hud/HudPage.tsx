import { useEffect, useState } from 'react';
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

/**
 * Whether the sequence has already had its one showing this process.
 *
 * Module scope, not component state, and that is the point: `HudPage`
 * unmounts whenever the overlay navigates to another tab and mounts again on
 * the way back, so anything held inside the component would reset and the
 * sequence would replay on every return.
 *
 * Once per process was the user's call, after seeing it run on every opening
 * and judging it not worth the 1.5s.
 */
let bootSequencePlayed = false;

/** Test-only. Nothing in the app resets this — a process gets one sequence. */
export function resetBootSequenceForTests(): void {
  bootSequencePlayed = false;
}

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
  const [booting, setBooting] = useState(!bootSequencePlayed);
  const visible = useSurfaceVisibility();

  // Gated on visibility because the window is born on a hidden Hyprland
  // special workspace: a sequence started at mount would play out behind the
  // curtain and never be seen. That gate only works because
  // `useSurfaceVisibility` has a signal that actually fires (see its comment)
  // — the webview's own `visibilityState` never changes here.
  //
  // Once per process. It ran on every opening for a while and the verdict was
  // that it is not worth 1.5s between the keystroke and the data.
  // No `bootSequencePlayed` check here on purpose: once it is set, `booting`
  // starts false and this timer's two writes are both no-ops that React bails
  // out of. Mutation testing showed the extra guard changed nothing any test
  // could see, and an unprovable branch is worse than the dead timer it saves.
  useEffect(() => {
    if (!visible) return;
    const t = setTimeout(() => {
      bootSequencePlayed = true;
      setBooting(false);
    }, BOOT_MS);
    return () => clearTimeout(t);
  }, [visible]);

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
