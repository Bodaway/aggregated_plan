import { useEffect, useRef, useState } from 'react';
import { useSurfaceVisibility } from './useSurfaceVisibility';
import { useDominantBlock } from './useDominantBlock';
import { useActiveBreak } from './useActiveBreak';
import { BreakScreen } from './BreakScreen';
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

/** How long `playing` stays on: the last panel's delay (430ms) plus its own
 *  duration (620ms), which also clears the 980ms sweep, plus a little. Slowed
 *  from 700ms on the user's read of it running — the first pass was over
 *  before it registered. Removing the attribute is what arms the next
 *  opening: the rules stop matching, so applying them again runs them from
 *  the top rather than finding an animation already spent. */
const OPENING_MS = 1150;

type OpenState = 'armed' | 'playing' | undefined;

/**
 * Drives the opening animation, in three states.
 *
 * The grid is rendered and readable throughout — this animates data in, it
 * never makes anyone wait for it. That distinction is the lesson of the boot
 * sequence, cut back to once per process for standing between the keystroke
 * and the numbers.
 *
 * `armed` exists because of a flicker the user caught: the compositor reveals
 * the window BEFORE the signal telling us it is revealed arrives, so a grid
 * that only starts animating on the signal is briefly seen finished, drops to
 * nothing and climbs back. Arming on HIDE puts it at the animation's first
 * frame while nobody is looking.
 *
 * Mount plays straight away rather than arming: the grid mounts when the boot
 * sequence ends, and again on every return from another tab, and both are
 * moments it is already in front of someone.
 */
function useOpeningAnimation(): OpenState {
  const visible = useSurfaceVisibility();
  const [state, setState] = useState<OpenState>('playing');
  const wasVisible = useRef(visible);

  useEffect(() => {
    const opened = visible && !wasVisible.current;
    const closed = !visible && wasVisible.current;
    wasVisible.current = visible;
    if (opened) setState('playing');
    if (closed) setState('armed');
  }, [visible]);

  // Safety net for the one failure `armed` could otherwise cause: a grid
  // parked invisible because the reveal signal never came (the workspace
  // toggled by something other than our own script, say). Focus is not a
  // reliable "visible", which is why it does not drive the state generally —
  // but it is a reliable "definitely on screen now", and the only thing it
  // can do here is reveal a grid that is already hidden.
  useEffect(() => {
    if (state !== 'armed') return;
    const reveal = () => setState('playing');
    window.addEventListener('focus', reveal);
    return () => window.removeEventListener('focus', reveal);
  }, [state]);

  useEffect(() => {
    if (state !== 'playing') return;
    const timer = setTimeout(() => setState(undefined), OPENING_MS);
    return () => clearTimeout(timer);
  }, [state]);

  return state;
}

/** The grid proper, split out of `HudPage` so that nothing behind the boot
 *  sequence touches the network: `useDominantBlock` reads the dashboard, and
 *  the six blocks each read their own hooks — all of it mounts when the boot
 *  animation ends, not while it plays. */
function HudGrid() {
  // One arbiter, one glow: every `lit` below is derived from this single
  // value, so no combination of props can light two panels at once.
  const dominant = useDominantBlock();
  const openState = useOpeningAnimation();

  return (
    <div data-testid="hud-grid" className="hud" data-open={openState}>
      <HudNav />
      <FocusBlock lit={dominant === 'focus'} />
      <PressureBlock lit={dominant === 'pressure'} />
      <AgendaBlock lit={dominant === 'agenda'} />
      <NeuralBudgetBlock />
      <AgentsBlock />
      <StationBlock />
      <Ticker />
      {openState === 'playing' && <div className="hud-sweep" data-testid="hud-sweep" />}
    </div>
  );
}

export function HudPage() {
  const [booting, setBooting] = useState(!bootSequencePlayed);
  const visible = useSurfaceVisibility();
  const activeBreak = useActiveBreak();

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
      {/* A break takes the surface ahead of everything, boot sequence
          included: the overlay was very likely opened BY the break, and 1.5s
          of curtain in front of a 30s countdown spends a twentieth of the
          break on a splash screen. The timer above is left running through
          it — skipping the sequence is not deferring it, so the grid is there
          the moment the break ends. */}
      {activeBreak ? (
        <BreakScreen session={activeBreak} />
      ) : booting ? (
        <pre data-testid="boot-sequence" className="p-8 text-sm text-cn-teal">
          {BOOT_LINES.join('\n')}
        </pre>
      ) : (
        <HudGrid />
      )}
    </div>
  );
}
