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

  test('search bar: "/" focuses input, typing shows suggestions, click opens edit sheet', async ({ page }) => {
    await page.goto('/dashboard');

    // Wait for at least one task card to render
    const firstCard = page.locator('[data-testid="task-card-root"]').first();
    await expect(firstCard).toBeVisible();

    // Grab a known title from a visible card
    const title = (await firstCard.textContent())?.trim().split('\n')[0] ?? '';
    expect(title.length).toBeGreaterThan(2);
    const needle = title.slice(0, 3);

    // "/" focuses the search input
    await page.keyboard.press('/');
    const input = page.getByRole('combobox');
    await expect(input).toBeFocused();

    // Typing shows the dropdown listbox
    await input.fill(needle);
    const listbox = page.getByRole('listbox');
    await expect(listbox).toBeVisible();

    // Picking the first suggestion opens the edit sheet
    await page.getByRole('option').first().click();
    // TaskEditSheet doesn't expose role="dialog" or data-sheet-open, so we assert the sheet is visible
    // by checking for the "Cancel" button which only appears when the sheet is open (isOpen && ...)
    await expect(page.getByRole('button', { name: /cancel/i })).toBeVisible({ timeout: 2000 });
  });
});
