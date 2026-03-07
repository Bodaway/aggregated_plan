import { BrowserRouter, Routes, Route, Navigate } from 'react-router-dom';
import { PageLayout } from '@/components/layout/PageLayout';

function PlaceholderContent({ name }: { readonly name: string }) {
  return (
    <div className="flex items-center justify-center h-full">
      <p className="text-gray-500 text-lg">{name} content coming soon</p>
    </div>
  );
}

export function App() {
  return (
    <BrowserRouter>
      <Routes>
        <Route path="/" element={<Navigate to="/dashboard" replace />} />
        <Route
          path="/dashboard"
          element={
            <PageLayout title="Dashboard">
              <PlaceholderContent name="Dashboard" />
            </PageLayout>
          }
        />
        <Route
          path="/priority"
          element={
            <PageLayout title="Priority Matrix">
              <PlaceholderContent name="Priority Matrix" />
            </PageLayout>
          }
        />
        <Route
          path="/workload"
          element={
            <PageLayout title="Workload">
              <PlaceholderContent name="Workload" />
            </PageLayout>
          }
        />
        <Route
          path="/activity"
          element={
            <PageLayout title="Activity Journal">
              <PlaceholderContent name="Activity Journal" />
            </PageLayout>
          }
        />
        <Route
          path="/settings"
          element={
            <PageLayout title="Settings">
              <PlaceholderContent name="Settings" />
            </PageLayout>
          }
        />
      </Routes>
    </BrowserRouter>
  );
}
