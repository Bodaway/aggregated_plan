import { test, expect } from '@playwright/test';

test.describe('Smoke Tests', () => {
  test('dashboard loads', async ({ page }) => {
    await page.goto('/dashboard');
    await expect(page).toHaveTitle(/Aggregated Plan/);
    await expect(page.getByText('Dashboard')).toBeVisible();
  });

  test('can navigate to priority matrix', async ({ page }) => {
    await page.goto('/dashboard');
    await page.getByRole('link', { name: /priority/i }).click();
    await expect(page).toHaveURL(/priority/);
    await expect(page.getByText('Do First')).toBeVisible();
    await expect(page.getByText('Schedule')).toBeVisible();
  });

  test('can navigate to workload', async ({ page }) => {
    await page.goto('/dashboard');
    await page.getByRole('link', { name: /workload/i }).click();
    await expect(page).toHaveURL(/workload/);
  });

  test('can navigate to activity journal', async ({ page }) => {
    await page.goto('/dashboard');
    await page.getByRole('link', { name: /activity/i }).click();
    await expect(page).toHaveURL(/activity/);
  });

  test('can navigate to settings', async ({ page }) => {
    await page.goto('/dashboard');
    await page.getByRole('link', { name: /settings/i }).click();
    await expect(page).toHaveURL(/settings/);
  });
});
