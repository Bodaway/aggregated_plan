import { useEffect, useState } from 'react';
import { useSession } from '@/hooks/use-session';

const LOGIN_URL = 'http://localhost:3001/auth/microsoft/login';

export function AuthGate({ children }: { children: React.ReactNode }) {
  const { session, fetching, refresh } = useSession();
  const [errorReason, setErrorReason] = useState<string | null>(null);

  useEffect(() => {
    const p = new URLSearchParams(window.location.search);
    const reason = p.get('reason');
    if (reason) {
      setErrorReason(reason.slice(0, 120));
    }
    window.history.replaceState({}, '', window.location.pathname);
    refresh();
  }, [refresh]);

  if (fetching) {
    return <div className="flex h-screen items-center justify-center text-sm text-muted-foreground">Loading…</div>;
  }
  if (!session.authenticated) {
    return (
      <div className="flex h-screen flex-col items-center justify-center gap-4">
        <h1 className="text-xl font-semibold">Aggregated Plan</h1>
        <p className="text-sm text-muted-foreground">Sign in with your Microsoft account to continue.</p>
        {errorReason && <p className="text-sm text-red-600">Sign-in failed: {errorReason}</p>}
        <a className="rounded bg-blue-600 px-4 py-2 text-white hover:bg-blue-700 transition-colors" href={LOGIN_URL}>
          Sign in with Microsoft
        </a>
      </div>
    );
  }
  return <>{children}</>;
}
