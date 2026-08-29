import { HeaderSearchBar } from '@/components/search/HeaderSearchBar';
import { useSession } from '@/hooks/use-session';

interface HeaderProps {
  readonly title: string;
}

export function Header({ title }: HeaderProps) {
  const { session, signOut } = useSession();

  return (
    <header className="app-head">
      <h2 className="app-head__title">{title}</h2>
      <div className="flex items-center gap-4">
        <HeaderSearchBar />
        {session.authenticated && (
          <div className="flex items-center gap-3">
            <span className="app-head__who">{session.account}</span>
            <button type="button" className="app-head__out" onClick={() => signOut()}>
              Sign out
            </button>
          </div>
        )}
      </div>
    </header>
  );
}
