import { useEffect, useState } from 'react';
import { useLocation } from 'react-router-dom';
import { Sidebar } from './Sidebar';
import { Header } from './Header';

interface PageLayoutProps {
  readonly title: string;
  readonly children: React.ReactNode;
}

/** How long the sweep runs over the page area. Matches the tail of the
 *  staggered entrance in `app-shell.css`, so both end together. */
const SWEEP_MS = 780;

export function PageLayout({ title, children }: PageLayoutProps) {
  const { pathname } = useLocation();
  const [sweeping, setSweeping] = useState(true);

  // Re-armed on every navigation. The staggered entrance needs no such thing
  // — its wrapper is keyed on the route, so it is a genuinely new element and
  // its animation runs from the top — but the sweep is one element that has
  // to be taken away and put back.
  useEffect(() => {
    setSweeping(true);
    const timer = setTimeout(() => setSweeping(false), SWEEP_MS);
    return () => clearTimeout(timer);
  }, [pathname]);

  return (
    <div className="app-shell">
      <Sidebar />
      <div className="app-main">
        <Header title={title} />
        <main className="app-body relative">
          {/* Keyed on the route: navigating replaces this wrapper rather than
              updating it, which is what makes the entrance play again. */}
          <div className="app-page" key={pathname}>
            {children}
          </div>
          {sweeping && <div className="app-sweep" data-testid="app-sweep" />}
        </main>
      </div>
    </div>
  );
}
