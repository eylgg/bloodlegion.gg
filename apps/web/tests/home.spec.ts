import { expect, test } from '@playwright/test';

test('the home page shows the logo', async ({ page }) => {
	await page.goto('/');
	await expect(page.getByRole('img', { name: 'Blood Legion' })).toBeVisible();
});

test('an unknown path is a 404 that still shows the logo', async ({ page }) => {
	const response = await page.goto('/not-a-page');
	expect(response?.status()).toBe(404);
	await expect(page.getByRole('img', { name: 'Blood Legion' })).toBeVisible();
	await expect(page.getByText('404: Not Found')).toBeVisible();
});
