<script lang="ts">
	import { page } from '$app/state';
	import type { PageProps } from './$types';
	import type { LoginProvider } from '$lib/types';
	import Logo from '$lib/components/Logo.svelte';
	import Button from '$lib/components/Button.svelte';
	import Alert from '$lib/components/Alert.svelte';
	import Announcements from '$lib/components/Announcements.svelte';
	import GuildShell from '$lib/components/guild/GuildShell.svelte';
	import Home from '$lib/components/guild/Home.svelte';
	import { providerVariant, returnedProviderName } from '$lib/auth';

	let { data }: PageProps = $props();

	// Only the providers that can make an account are offered (Battle.net). Connection-only ones
	// (Discord) are never a way in: they are linked from the profile.
	const providers = $derived(
		data.providers.filter(
			(p): p is Exclude<LoginProvider, { kind: 'local' }> =>
				p.kind === 'oauth2' && !p.connection_only && p.registration_allowed
		)
	);

	// A sign-in that was sent here from elsewhere (the OAuth2 consent screen) returns there after.
	// The backend validates `next` and falls back to `/` if it is unsafe.
	// After a permissions failure, the retry asks the provider to show its consent screen again:
	// otherwise it silently re-grants whatever was allowed the first time and fails the same way.
	const nextQuery = $derived.by(() => {
		const parts: string[] = [];
		const next = page.url.searchParams.get('next');
		if (next) parts.push(`next=${encodeURIComponent(next)}`);
		if (page.url.searchParams.get('error') === 'permissions_required') {
			parts.push('prompt=consent');
		}
		return parts.length ? `?${parts.join('&')}` : '';
	});

	// A failed sign-in comes back as `/?error=<code>&provider=<slug>` (see the backend's
	// `ProviderError::code`). Only known codes get their own words, naming only a listed provider;
	// anything else is the generic message, so a crafted link cannot put arbitrary text on the page.
	const loginError = $derived.by(() => {
		const code = page.url.searchParams.get('error');
		if (!code) return null;
		const provider = returnedProviderName(data.providers, page.url) ?? 'the provider';
		const errors: Record<string, string> = {
			// A provider (Battle.net) may remember what was allowed the first time and silently
			// re-grant exactly that, ignoring requests to ask again, so "try again" alone loops; the
			// reset is on their side.
			permissions_required:
				`Signing in needs every permission the site asks for, and ${provider} remembers what ` +
				`you allowed before. Remove Blood Legion from the applications connected to your ` +
				`account at ${provider}, then sign in again and leave every permission ticked.`,
			unlinked:
				`That ${provider} account was unlinked. Sign in another way, then link it again ` +
				'from your profile.',
			cancelled: 'Sign-in was cancelled.',
			expired: 'That sign-in took too long or was already used. Please try again.',
			verification_failed: 'We could not verify your sign-in. Please try again.',
			registration_closed: 'New accounts are not being created right now.',
			email_in_use: 'That email address already belongs to another account.',
			unknown_provider: 'That sign-in method is not available.',
			account_failed: 'Your account could not be created. Please try again.',
			unavailable: 'Sign-in is unavailable right now. Please try again later.'
		};
		return errors[code] ?? 'Sign-in failed. Please try again.';
	});
</script>

{#snippet announcements()}
	<Announcements announcements={data.announcements} />
{/snippet}

<svelte:head>
	<title>Blood Legion</title>
</svelte:head>

{#if data.home}
	<!-- A member signed in: their own front page, inside the guild's pages with none picked. -->
	<GuildShell user={data.home.user}>
		<Home
			zones={data.zones}
			calendar={data.home.calendar}
			member={data.home.member}
			raids={data.home.raids}
			loot={data.home.loot}
			announcements={data.announcements}
		/>
	</GuildShell>
{:else}
	<Logo after={data.announcements.length > 0 ? announcements : undefined}>
		{#if loginError}
			<Alert variant="error">{loginError}</Alert>
		{/if}
		{#each providers as provider (provider.slug)}
			<!-- Battle.net gets its own blue (see `providerVariant`). The link leaves
			     the app for the backend (which redirects to the provider), so it must be a full page
			     load: `data-sveltekit-reload` stops the client router from treating `/api/...` as one
			     of its own pages and rendering its 404. -->
			<Button
				data-sveltekit-reload
				href={`/api/auth/oauth2/providers/${provider.slug}${nextQuery}`}
				variant={providerVariant(provider.slug)}
				full
			>
				Log in with {provider.name}
			</Button>
		{/each}
	</Logo>
{/if}
