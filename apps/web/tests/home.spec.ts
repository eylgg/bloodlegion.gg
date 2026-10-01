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

test('without a backend the home page is anonymous and sign-in reports itself unavailable', async ({
	page
}) => {
	await page.goto('/');
	await expect(page.getByRole('link', { name: 'Sign in' })).toBeVisible();
	const response = await page.goto('/login');
	expect(response?.status()).toBe(503);
	await expect(page.getByRole('img', { name: 'Blood Legion' })).toBeVisible();
	await expect(page.getByText('503: Sign-in is unavailable right now.')).toBeVisible();
});

test('the registration page needs a pending sign-in', async ({ page }) => {
	const response = await page.goto('/register');
	expect(response?.status()).toBe(503);
	await expect(page.getByRole('img', { name: 'Blood Legion' })).toBeVisible();
});
