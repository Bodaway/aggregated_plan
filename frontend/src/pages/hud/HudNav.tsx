import { useEffect } from 'react';
import { Link, useLocation, useNavigate } from 'react-router-dom';
import { isTypingTarget } from '@/lib/is-typing-target';

/**
 * The eleven real destinations of the app, in the order the mockup lays
 * them out, plus the HUD itself. `key` is the one-key shortcut printed next
 * to each label (digits 1-9, then `a` and `,` once the digits run out) —
 * the same mapping the mockup uses.
 */
interface Destination {
  readonly key: string;
  readonly path: string;
  readonly label: string;
}

const DESTINATIONS: readonly Destination[] = [
  { key: '0', path: '/hud', label: 'HUD' },
  { key: '1', path: '/dashboard', label: 'Dashboard' },
  { key: '2', path: '/triage', label: 'Triage' },
  { key: '3', path: '/priority', label: 'Priority' },
  { key: '4', path: '/workload', label: 'Workload' },
  { key: '5', path: '/activity', label: 'Activity' },
  { key: '6', path: '/timesheet', label: 'Timesheet' },
  { key: '7', path: '/worklog', label: 'Worklog' },
  { key: '8', path: '/memory', label: 'Memory' },
  { key: '9', path: '/dedup', label: 'Dedup' },
  { key: 'a', path: '/alerts', label: 'Alerts' },
  { key: ',', path: '/settings', label: 'Settings' },
];

export function HudNav() {
  const location = useLocation();
  const navigate = useNavigate();

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.metaKey || e.ctrlKey || e.altKey) return;
      if (isTypingTarget(document.activeElement)) return;
      const dest = DESTINATIONS.find((d) => d.key === e.key.toLowerCase());
      if (dest) {
        e.preventDefault();
        navigate(dest.path);
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [navigate]);

  return (
    <nav className="hud-nav">
      <span className="hud-nav__mark">APLAN</span>
      <div className="hud-nav__dest">
        {DESTINATIONS.map((dest) => {
          const current = location.pathname === dest.path;
          return (
            <Link
              key={dest.path}
              to={dest.path}
              aria-current={current ? 'page' : undefined}
              className={
                current ? 'hud-nav__link hud-nav__link--current' : 'hud-nav__link'
              }
            >
              <i className="hud-nav__digit">{dest.key.toUpperCase()}</i>
              {dest.label}
            </Link>
          );
        })}
      </div>
      <span className="hud-nav__esc">Esc closes</span>
    </nav>
  );
}
