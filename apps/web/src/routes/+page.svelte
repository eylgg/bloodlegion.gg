<script lang="ts">
	import { page } from '$app/state';
	import type { PageProps } from './$types';
	import type { LoginProvider } from '$lib/types';
	import Logo from '$lib/components/Logo.svelte';
	import Button from '$lib/components/Button.svelte';
	import Alert from '$lib/components/Alert.svelte';
	import { api } from '$lib/api';

	let { data }: PageProps = $props();
	let signingOut = $state(false);

	const providers = $derived(
		data.providers.filter(
			(p): p is Exclude<LoginProvider, { kind: 'local' }> => p.kind === 'oauth2'
		)
	);

	// A sign-in that was sent here from elsewhere (the OAuth2 consent screen) returns there after.
	// The backend validates `next` and falls back to `/` if it is unsafe.
	const nextQuery = $derived.by(() => {
		const next = page.url.searchParams.get('next');
		return next ? `?next=${encodeURIComponent(next)}` : '';
	});

	// A failed sign-in comes back as `/?error=<code>` (see the backend's `ProviderError::code`).
	// Only known codes get their own words; anything else is the generic message, so a crafted
	// link cannot put arbitrary text on the page.
	const ERRORS: Record<string, string> = {
		permissions_required:
			'Signing in needs every permission the site asks for. Please try again and allow them all.',
		cancelled: 'Sign-in was cancelled.',
		expired: 'That sign-in took too long or was already used. Please try again.',
		verification_failed: 'We could not verify your sign-in. Please try again.',
		registration_closed: 'New accounts are not being created right now.',
		email_in_use: 'That email address already belongs to another account.',
		unknown_provider: 'That sign-in method is not available.',
		account_failed: 'Your account could not be created. Please try again.',
		unavailable: 'Sign-in is unavailable right now. Please try again later.'
	};
	const loginError = $derived.by(() => {
		const code = page.url.searchParams.get('error');
		if (!code) return null;
		return ERRORS[code] ?? 'Sign-in failed. Please try again.';
	});

	async function signOut() {
		signingOut = true;
		try {
			await api.post('/api/auth/logout');
		} finally {
			window.location.assign('/');
		}
	}
</script>

<Logo>
	{#if !data.user && loginError}
		<Alert variant="error">{loginError}</Alert>
	{/if}
	{#if !data.user}
		{#each providers as provider (provider.slug)}
			<!-- Battle.net gets its own blue; any other provider the house style. The link leaves
			     the app for the backend (which redirects to the provider), so it must be a full page
			     load: `data-sveltekit-reload` stops the client router from treating `/api/...` as one
			     of its own pages and rendering its 404. -->
			<Button
				data-sveltekit-reload
				href={`/api/auth/oauth2/providers/${provider.slug}${nextQuery}`}
				variant={provider.slug === 'battlenet' ? 'battlenet' : 'secondary'}
				full
			>
				Log in with {provider.name}
			</Button>
		{/each}
	{/if}
</Logo>

{#if data.user}
	<!-- Who is signed in, and the way out. -->
	<footer class="account">
		<span>{data.user.username}</span>
		<span class="sep" aria-hidden="true">·</span>
		<button type="button" onclick={signOut} disabled={signingOut}>Sign out</button>
	</footer>
{/if}

<style>
	.account {
		position: fixed;
		inset: auto 0 var(--space-6);
		display: flex;
		justify-content: center;
		gap: var(--space-2);
		color: var(--grey-text);
		font-family: var(--font-mono);
		font-size: var(--text-sm);
	}

	.sep {
		color: var(--grey-soft);
	}

	button {
		color: inherit;
		background: none;
		border: 0;
		padding: 0;
		font: inherit;
		cursor: pointer;
		text-decoration: underline;
		text-underline-offset: 0.2em;
	}

	button:hover {
		color: var(--grey-text-active);
	}
</style>
