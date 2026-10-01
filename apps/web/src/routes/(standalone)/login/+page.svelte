<script lang="ts">
	import { page } from '$app/state';
	import type { PageProps } from './$types';
	import type { LoginProvider } from '$lib/types';
	import { api, ApiError, errorMessage as problemMessage } from '$lib/api';
	import Button from '$lib/components/Button.svelte';
	import Card from '$lib/components/Card.svelte';
	import Input from '$lib/components/Input.svelte';
	import Alert from '$lib/components/Alert.svelte';
	import FormActions from '$lib/components/FormActions.svelte';

	let { data }: PageProps = $props();

	let error = $state('');
	let submitting = $state(false);

	const localEnabled = $derived(data.options.providers.some((p) => p.kind === 'local'));
	const externalProviders = $derived(
		data.options.providers.filter(
			(p): p is Exclude<LoginProvider, { kind: 'local' }> => p.kind !== 'local'
		)
	);

	// Forward the raw `next` to the backend (it validates and falls back); the URL transport is
	// our only concern, so percent-encode the whole value.
	function nextQuery(): string {
		const next = page.url.searchParams.get('next');
		return next ? `?next=${encodeURIComponent(next)}` : '';
	}

	async function handleLogin(event: SubmitEvent) {
		event.preventDefault();
		submitting = true;
		error = '';

		try {
			const form = event.currentTarget as HTMLFormElement;
			const formData = new FormData(form);
			const { url } = await api.post<{ url: string }>(`/api/auth/local/login${nextQuery()}`, {
				username: String(formData.get('username') ?? ''),
				password: String(formData.get('password') ?? '')
			});
			window.location.assign(url);
			return;
		} catch (err) {
			if (err instanceof ApiError && err.status === 401) {
				error = err.problem.detail || 'Invalid username or password.';
			} else if (err instanceof ApiError && err.status >= 500) {
				error = 'Internal server error. Please try again later.';
			} else {
				error = problemMessage(err, 'An unknown error occurred. Please try again later.');
			}
		} finally {
			submitting = false;
		}
	}

	function loginWithOauth2(slug: string) {
		submitting = true;
		window.location.assign(`/api/auth/oauth2/providers/${slug}${nextQuery()}`);
	}
</script>

<svelte:head>
	<title>Sign in | Blood Legion</title>
</svelte:head>

<Card>
	<h1>Sign in</h1>

	{#if error}
		<Alert variant="error">{error}</Alert>
	{/if}

	{#if externalProviders.length > 0}
		<div class="providers">
			{#each externalProviders as provider (provider.slug)}
				<Button
					type="button"
					variant={provider.slug === 'battlenet' ? 'battlenet' : 'primary'}
					full
					disabled={submitting}
					onclick={() => loginWithOauth2(provider.slug)}
				>
					Log in with {provider.name}
				</Button>
			{/each}
		</div>
	{/if}

	{#if localEnabled && externalProviders.length > 0}
		<div class="divider"><span>or</span></div>
	{/if}

	{#if localEnabled}
		<form class="login-form" onsubmit={handleLogin}>
			<Input
				label="Username"
				name="username"
				type="text"
				required
				minlength={3}
				maxlength={32}
				autocomplete="username"
			/>
			<Input
				label="Password"
				name="password"
				type="password"
				required
				autocomplete="current-password"
			/>
			<FormActions align="end">
				<Button
					type="submit"
					variant={externalProviders.length > 0 ? 'secondary' : 'primary'}
					disabled={submitting}
				>
					{submitting ? 'Signing in...' : 'Sign in'}
				</Button>
			</FormActions>
		</form>
	{/if}

	{#if !localEnabled && externalProviders.length === 0}
		<p class="muted">No sign-in methods are configured. Contact an administrator.</p>
	{/if}
</Card>

<style>
	.login-form,
	.providers {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
	}

	.muted {
		color: var(--grey-text);
		font-size: var(--text-md);
	}

	/* "or" separator with a rule on each side. */
	.divider {
		display: flex;
		align-items: center;
		gap: var(--space-3);
		color: var(--grey-text);
		font-size: var(--text-xs);
		text-transform: uppercase;
	}

	.divider::before,
	.divider::after {
		content: '';
		flex: 1;
		height: 1px;
		background-color: var(--grey-surface);
	}
</style>
