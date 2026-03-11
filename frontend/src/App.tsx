import { BrowserRouter, Routes, Route, Navigate } from 'react-router-dom';
import { PageLayout } from '@/components/layout/PageLayout';
import { DashboardPage } from '@/pages/DashboardPage';
import { PriorityMatrixPage } from '@/pages/PriorityMatrixPage';
import { WorkloadPage } from '@/pages/WorkloadPage';
import { ActivityJournalPage } from '@/pages/ActivityJournalPage';
import { DeduplicationPage } from '@/pages/DeduplicationPage';
import { AlertsPage } from '@/pages/AlertsPage';
import { SettingsPage } from '@/pages/SettingsPage';
import { TriagePage } from '@/pages/TriagePage';

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
          path="/triage"
          element={
            <PageLayout title="Triage">
              <TriagePage />
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
              <ActivityJournalPage />
            </PageLayout>
          }
        />
        <Route
          path="/dedup"
          element={
            <PageLayout title="Deduplication">
              <DeduplicationPage />
            </PageLayout>
          }
        />
        <Route
          path="/alerts"
          element={
            <PageLayout title="Alerts">
              <AlertsPage />
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
