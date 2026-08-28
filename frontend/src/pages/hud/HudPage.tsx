import { useEffect, useState } from 'react';
import { useSurfaceVisibility } from './useSurfaceVisibility';
import { HudNav } from './HudNav';
import { FocusBlock } from './blocks/FocusBlock';
import { PressureBlock } from './blocks/PressureBlock';
import { AgendaBlock } from './blocks/AgendaBlock';
import { NeuralBudgetBlock } from './blocks/NeuralBudgetBlock';
import { AgentsBlock } from './blocks/AgentsBlock';
import { StationBlock } from './blocks/StationBlock';
import { Ticker } from './blocks/Ticker';
import './hud.css';

const BOOT_LINES = [
  'aplan cockpit v0.1.0',
  'link 127.0.0.1:3001 ......... ok',
  'palette cybernord .......... ok',
  'session bus ................ ok',
] as const;

const BOOT_MS = 1500;

export function HudPage() {
  const [booting, setBooting] = useState(true);
  const visible = useSurfaceVisibility();

  useEffect(() => {
    const t = setTimeout(() => setBooting(false), BOOT_MS);
    return () => clearTimeout(t);
  }, []);

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
        <div data-testid="hud-grid" className="hud">
          <HudNav />
          <FocusBlock lit />
          <PressureBlock />
          <AgendaBlock />
          <NeuralBudgetBlock />
          <AgentsBlock />
          <StationBlock />
          <Ticker />
        </div>
      )}
    </div>
  );
}
