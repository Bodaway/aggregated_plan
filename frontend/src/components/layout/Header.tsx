import { HeaderSearchBar } from '@/components/search/HeaderSearchBar';
import { useSession } from '@/hooks/use-session';

interface HeaderProps {
  readonly title: string;
}

export function Header({ title }: HeaderProps) {
  const { session, signOut } = useSession();

  return (
    <header className="flex items-center justify-between bg-white border-b border-gray-200 px-6 py-4">
      <h2 className="text-lg font-semibold text-gray-800">{title}</h2>
      <div className="flex items-center gap-4">
        <HeaderSearchBar />
        {session.authenticated && (
          <div className="flex items-center gap-2 text-sm">
            <span className="text-muted-foreground text-gray-500">{session.account}</span>
            <button
              type="button"
              className="rounded border border-gray-300 px-2 py-1 text-xs hover:bg-gray-50 transition-colors"
              onClick={() => signOut()}
            >
              Sign out
            </button>
          </div>
        )}
      </div>
    </header>
  );
}
