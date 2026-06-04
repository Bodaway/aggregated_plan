import { test, expect } from '@playwright/test';

test.describe('Worklog feature', () => {
  test('add an entry from a task and see it on the Worklog page', async ({ page }) => {
    // Unique marker so the test is idempotent across runs.
    const marker = `E2E worklog ${Date.now()}`;

    // 1. Open a task via the search bar → edit sheet opens.
    await page.goto('/dashboard');

    const firstCard = page.locator('[data-testid="task-card-root"]').first();
    await expect(firstCard).toBeVisible();

    const title = (await firstCard.textContent())?.trim().split('\n')[0] ?? '';
    expect(title.length).toBeGreaterThan(2);
    const needle = title.slice(0, 3);

    await page.keyboard.press('/');
    const input = page.getByRole('combobox');
    await expect(input).toBeFocused();
    await input.fill(needle);
    await expect(page.getByRole('listbox')).toBeVisible();
    await page.getByRole('option').first().click();
    await expect(page.getByTestId('task-sheet-cancel')).toBeVisible({ timeout: 2000 });

    // 2. Log an entry from the Worklog section.
    await expect(page.getByRole('heading', { name: /worklog/i })).toBeVisible();
    const textarea = page.getByPlaceholder(/log an entry/i);
    await textarea.fill(marker);
    await page.getByRole('button', { name: /log entry/i }).click();

    // Entry appears in the sheet.
    await expect(page.getByText(marker)).toBeVisible();

    // 3. Navigate to /worklog and confirm the entry appears there too.
    await page.goto('/worklog');
    await expect(page.getByText(marker)).toBeVisible();
  });
});
