import { useEffect, useState } from 'react';
import { useSurfaceVisibility } from './useSurfaceVisibility';

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
      className="h-screen w-screen bg-transparent font-cn text-cn-fg"
      data-surface-visible={visible}
    >
      {booting ? (
        <pre data-testid="boot-sequence" className="p-8 text-sm text-cn-teal">
          {BOOT_LINES.join('\n')}
        </pre>
      ) : (
        <div data-testid="hud-grid" className="grid h-full grid-cols-12 gap-3 p-6">
          {/* The six blocks arrive in plan 3. */}
        </div>
      )}
    </div>
  );
}
