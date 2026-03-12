interface HeaderProps {
  readonly title: string;
}

export function Header({ title }: HeaderProps) {
  return (
    <header className="bg-white border-b border-gray-200 px-6 py-4">
      <h2 className="text-lg font-semibold text-gray-800">{title}</h2>
    </header>
  );
}
