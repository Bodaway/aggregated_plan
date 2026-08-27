import { useEffect, useState } from 'react';

/**
 * True while the HUD surface is actually being looked at.
 *
 * Every animation in the HUD must be gated on this. The design benchmark showed
 * that continuous animation accounts for ~99% of CPU cost, and that no native
 * toolkit stops on its own when covered — so we stop explicitly.
 */
export function useSurfaceVisibility(): boolean {
  const [visible, setVisible] = useState(() => document.visibilityState === 'visible');

  useEffect(() => {
    const onChange = () => setVisible(document.visibilityState === 'visible');
    document.addEventListener('visibilitychange', onChange);
    return () => document.removeEventListener('visibilitychange', onChange);
  }, []);

  return visible;
}
