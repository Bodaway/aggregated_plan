import { BrowserRouter, Routes, Route, Navigate } from 'react-router-dom';

function PlaceholderPage({ name }: { name: string }) {
  return (
    <div className="flex items-center justify-center min-h-screen">
      <h1 className="text-2xl font-bold text-gray-800">{name}</h1>
    </div>
  );
}

export function App() {
  return (
    <BrowserRouter>
      <Routes>
        <Route path="/" element={<Navigate to="/dashboard" replace />} />
        <Route path="/dashboard" element={<PlaceholderPage name="Dashboard" />} />
        <Route path="/priority" element={<PlaceholderPage name="Priority Matrix" />} />
        <Route path="/workload" element={<PlaceholderPage name="Workload" />} />
        <Route path="/activity" element={<PlaceholderPage name="Activity Journal" />} />
        <Route path="/settings" element={<PlaceholderPage name="Settings" />} />
      </Routes>
    </BrowserRouter>
  );
}
