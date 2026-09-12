import type { ReactNode } from 'react';
import { NavLink } from 'react-router-dom';
import './mobile.css';

interface MobileShellProps {
  readonly children: ReactNode;
  /**
   * Nombre de captures en attente d'envoi. Le badge reste affiché en
   * permanence (même à zéro) : c'est ce qui rassure que rien n'a été perdu,
   * pas seulement ce qui alerte qu'il reste du travail.
   */
  readonly pendingCount?: number;
}

/**
 * Gabarit partagé des deux écrans mobiles (plan du jour, capture) : zone de
 * sécurité iPhone, navigation basse et badge de file d'attente. Isolé de
 * `PageLayout` (desktop) exactement comme `/hud` -- le tunnel Tailscale ne
 * doit jamais rendre la barre latérale desktop sur un écran de 390px.
 */
export function MobileShell({ children, pendingCount = 0 }: MobileShellProps) {
  return (
    <div className="mobile-shell">
      <header className="mobile-shell__header">
        <span className="mobile-shell__badge">{pendingCount} en attente</span>
      </header>
      <main className="mobile-shell__content mobile-page">{children}</main>
      <nav className="mobile-shell__nav">
        <NavLink to="/m" end className="mobile-shell__nav-link">
          Aujourd’hui
        </NavLink>
        <NavLink to="/m/new" className="mobile-shell__nav-link">
          Capturer
        </NavLink>
      </nav>
    </div>
  );
}
