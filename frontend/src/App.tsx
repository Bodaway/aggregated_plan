import { BrowserRouter, Routes, Route, Navigate } from 'react-router-dom';
import { PageLayout } from '@/components/layout/PageLayout';
import { DashboardPage } from '@/pages/DashboardPage';
import { PriorityMatrixPage } from '@/pages/PriorityMatrixPage';
import { WorkloadPage } from '@/pages/WorkloadPage';
import { SettingsPage } from '@/pages/SettingsPage';

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
              <DashboardPage />
            </PageLayout>
          }
        />
        <Route
          path="/priority"
          element={
            <PageLayout title="Priority Matrix">
              <PriorityMatrixPage />
            </PageLayout>
          }
        />
        <Route
          path="/workload"
          element={
            <PageLayout title="Workload">
              <WorkloadPage />
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
              <SettingsPage />
            </PageLayout>
          }
        />
      </Routes>
    </BrowserRouter>
  );
}
