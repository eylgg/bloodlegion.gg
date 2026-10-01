<script lang="ts">
	import { page } from '$app/state';
	import type { PageProps } from './$types';
	import type { LoginProvider } from '$lib/types';
	import Logo from '$lib/components/Logo.svelte';
	import Button from '$lib/components/Button.svelte';
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
