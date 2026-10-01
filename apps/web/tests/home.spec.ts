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

test('without a backend the home page is just the logo', async ({ page }) => {
	await page.goto('/');
	await expect(page.getByRole('img', { name: 'Blood Legion' })).toBeVisible();
	await expect(page.getByRole('link', { name: /Log in with/ })).toHaveCount(0);
});

test('there is no separate sign-in page', async ({ page }) => {
	const response = await page.goto('/login');
	expect(response?.status()).toBe(404);
});

test('the registration page needs a pending sign-in', async ({ page }) => {
	const response = await page.goto('/register');
	expect(response?.status()).toBe(503);
	await expect(page.getByRole('img', { name: 'Blood Legion' })).toBeVisible();
});

test('a failed sign-in shows a readable message, never text from the link', async ({ page }) => {
	await page.goto('/?error=permissions_required');
	await expect(page.getByRole('alert')).toContainText('every permission');
	await page.goto('/?error=<b>anything</b>');
	await expect(page.getByRole('alert')).toHaveText('Sign-in failed. Please try again.');
});
