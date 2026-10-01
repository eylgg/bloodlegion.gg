<script lang="ts">
	import type { PageProps } from './$types';
	import type { LoginProvider } from '$lib/types';
	import Logo from '$lib/components/Logo.svelte';
	import Button from '$lib/components/Button.svelte';
	import { api } from '$lib/api';
	import { resolve } from '$app/paths';

	let { data }: PageProps = $props();
	let signingOut = $state(false);

	const oauth2 = $derived(
		data.providers.filter(
			(p): p is Exclude<LoginProvider, { kind: 'local' }> => p.kind === 'oauth2'
		)
	);
	const localEnabled = $derived(data.providers.some((p) => p.kind === 'local'));

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
		{#each oauth2 as provider (provider.slug)}
			<!-- Battle.net gets its own blue; any other provider the house style. -->
			<Button
				href={`/api/auth/oauth2/providers/${provider.slug}`}
				variant={provider.slug === 'battlenet' ? 'battlenet' : 'secondary'}
				full
			>
				Log in with {provider.name}
			</Button>
		{/each}
	{/if}
</Logo>

<!-- The one line of chrome: who is signed in and the way out, or the fallback ways in. -->
<footer class="account">
	{#if data.user}
		<span>{data.user.username}</span>
		<span class="sep" aria-hidden="true">·</span>
		<button type="button" onclick={signOut} disabled={signingOut}>Sign out</button>
	{:else if localEnabled}
		<a href={resolve('/login')}>Sign in with a password</a>
	{:else if oauth2.length === 0}
		<a href={resolve('/login')}>Sign in</a>
	{/if}
</footer>

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

	a,
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

	a:hover,
	button:hover {
		color: var(--grey-text-active);
	}
</style>
