<script lang="ts">
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import { api, errorMessage } from '$lib/api';
	import { RANK_LABEL, formatDate } from '$lib/guild';
	import { providerVariant, returnedProviderName } from '$lib/auth';
	import Button from '$lib/components/Button.svelte';
	import Alert from '$lib/components/Alert.svelte';
	import ConfirmButton from '$lib/components/ConfirmButton.svelte';
	import CharacterForm from '$lib/components/guild/CharacterForm.svelte';
	import CharacterLink from '$lib/components/guild/CharacterLink.svelte';
	import CharacterSpecs from '$lib/components/guild/CharacterSpecs.svelte';
	import type { GuildCharacter, LinkedAccount } from '$lib/types';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	// svelte-ignore state_referenced_locally
	let accounts = $state<LinkedAccount[]>(data.accounts);
	// svelte-ignore state_referenced_locally
	let characters = $state<GuildCharacter[]>(data.characters);
	let error = $state('');

	// A link comes back as `/profile?linked=new|existing` or `/profile?error=<code>`, with
	// `&provider=<slug>` (see the backend's `links` module and `ProviderError::code`). Known codes
	// only, naming only a listed provider, so a crafted link cannot put arbitrary words on the page.
	const linked = $derived(page.url.searchParams.get('linked'));
	const linkedProvider = $derived(returnedProviderName(data.providers, page.url));
	// The linked accounts that are ways in: any not at a connection-only provider (Discord).
	const waysIn = $derived(
		accounts.filter((a) => !data.providers.find((p) => p.slug === a.provider_slug)?.connection_only)
			.length
	);
	// Which provider's section the outcome shows in: the one it came back from, when listed.
	const linkedSlug = $derived(
		data.providers.find((p) => p.slug === page.url.searchParams.get('provider'))?.slug ?? null
	);
	const linkError = $derived.by(() => {
		const code = page.url.searchParams.get('error');
		if (!code) return null;
		const provider = linkedProvider ?? 'the provider';
		const errors: Record<string, string> = {
			already_linked:
				`That ${linkedProvider ? `${linkedProvider} ` : ''}account is already linked to ` +
				'another account.',
			one_account_only:
				`You already have a ${provider} account linked, and only one can be. Unlink it first ` +
				'to link a different one.',
			permissions_required:
				`Linking needs every permission the site asks for, and ${provider} remembers what you ` +
				`allowed before. Remove Blood Legion from the applications connected to that account ` +
				`at ${provider}, then link it again and leave every permission ticked.`,
			cancelled: 'Linking was cancelled.',
			expired: 'That took too long or was already used. Please try again.',
			verification_failed:
				`We could not verify that ${linkedProvider ? `${linkedProvider} ` : ''}account. ` +
				'Please try again.',
			unknown_provider: 'That sign-in method is not available.',
			unavailable: 'Linking is unavailable right now. Please try again later.'
		};
		return errors[code] ?? 'Linking failed. Please try again.';
	});

	async function unlink(account: LinkedAccount) {
		error = '';
		try {
			await api.del(`/api/auth/linked-accounts/${account.id}`);
			accounts = await api.get<LinkedAccount[]>('/api/auth/linked-accounts');
		} catch (err) {
			error = errorMessage(err, 'Unlinking failed. Please try again.');
		}
	}

	// The character editor: `editing` is the character being changed, or null when adding.
	let open = $state(false);
	let editing = $state<GuildCharacter | null>(null);

	function sortCharacters(list: GuildCharacter[]) {
		return [...list].sort(
			(a, b) =>
				Number(b.is_main) - Number(a.is_main) ||
				`${a.first_name} ${a.last_name}`.localeCompare(`${b.first_name} ${b.last_name}`)
		);
	}

	function saved(character: GuildCharacter) {
		// Saving a main demotes the others on the server; mirror that here.
		const others = characters
			.filter((c) => c.id !== character.id)
			.map((c) => (character.is_main ? { ...c, is_main: false } : c));
		characters = sortCharacters([...others, character]);
		open = false;
	}

	async function makeMain(character: GuildCharacter) {
		error = '';
		try {
			saved(
				await api.put<GuildCharacter>(`/api/characters/${character.id}`, {
					first_name: character.first_name,
					last_name: character.last_name,
					class: character.class,
					race: character.race,
					is_main: true
				})
			);
		} catch (err) {
			error = errorMessage(err, 'That did not work. Please try again.');
		}
	}

	async function remove(character: GuildCharacter) {
		error = '';
		try {
			await api.del(`/api/characters/${character.id}`);
			characters = characters.filter((c) => c.id !== character.id);
		} catch (err) {
			error = errorMessage(err, 'Removing failed. Please try again.');
		}
	}
</script>

<svelte:head>
	<title>Profile | Blood Legion</title>
</svelte:head>

<div class="page-head">
	<div>
		<p class="kicker">
			{RANK_LABEL[data.user.guild_rank]}{data.user.is_superuser ? ' · Admin' : ''}
		</p>
		<h1>{data.user.username}</h1>
	</div>
</div>

{#if error}<Alert variant="error">{error}</Alert>{/if}

{#if !data.member}
	<Alert variant="info">
		Your account is public until an officer gives you a rank in the guild. Then the guild's pages
		open up. Link Discord below so they know who you are.
	</Alert>
{/if}

<!-- How a link came out, in the section of the provider it came back from (or above them all
     when that is not known). -->
{#snippet outcome()}
	{#if linked === 'new'}
		<Alert variant="success">Account linked.</Alert>
	{:else if linked === 'existing'}
		<Alert variant="info">
			{linkedProvider ?? 'The provider'} signed you straight back into an account that is already linked.
			To link a different one, sign out at {linkedProvider ?? 'the provider'} first (or pick another account
			when it asks), then try again.
		</Alert>
	{/if}
	{#if linkError}<Alert variant="error">{linkError}</Alert>{/if}
{/snippet}

{#if !linkedSlug}{@render outcome()}{/if}

<!-- One section per provider: Battle.net takes every account a member plays on; a
     provider without multiple connections (Discord) takes one, and offers no second once it has it. -->
{#each data.providers as provider (provider.slug)}
	{@const linkedHere = accounts.filter((a) => a.provider_slug === provider.slug)}
	<section>
		<h2>{provider.name}</h2>
		{#if linkedSlug === provider.slug}{@render outcome()}{/if}

		<ul class="accounts">
			{#each linkedHere as account (account.id)}
				<li>
					<div>
						<span class="identity">{account.identity ?? 'Unnamed account'}</span>
						<span class="muted">Linked {formatDate(account.linked_at)}</span>
					</div>
					{#if account.can_unlink}
						<ConfirmButton
							size="small"
							variant="danger"
							question="Unlink {account.identity ??
								`this ${account.provider_name} account`}? You will no longer be able to sign in with it."
							confirmLabel="Unlink"
							onconfirm={() => unlink(account)}>Unlink</ConfirmButton
						>
					{/if}
				</li>
			{:else}
				<li class="muted">Not linked.</li>
			{/each}
		</ul>
		<!-- Why a sign-in account has no Unlink. A connection-only one (Discord) is never a way in,
		     so the reasons do not apply to it. -->
		{#if !provider.connection_only && linkedHere.length > 0 && !linkedHere.some((a) => a.can_unlink)}
			<p class="muted small">
				{waysIn === 1
					? `Your only ${provider.name} account is how you sign in, so it cannot be unlinked.`
					: `Unlinking ${provider.name} accounts is turned off for now.`}
			</p>
		{/if}

		{#if provider.multiple_connections_allowed || linkedHere.length === 0}
			<div class="link-buttons">
				<!-- A full page load: the backend sends the browser on to the provider. -->
				<Button
					data-sveltekit-reload
					href={`/api/auth/oauth2/providers/${provider.slug}/link`}
					variant={providerVariant(provider.slug)}
				>
					{#if provider.multiple_connections_allowed}
						Link {linkedHere.length > 0 ? 'another' : 'a'} {provider.name} account
					{:else}
						Link your {provider.name} account
					{/if}
				</Button>
			</div>
		{/if}
	</section>
{/each}

{#if data.member}<section>
		<h2>Your characters</h2>
		<p class="muted">
			First and last name, as they are in game. Officers record loot and attendance against these{#if !data.officer},
				and add or remove them for you{/if}.
		</p>

		<ul class="characters">
			{#each characters as character (character.id)}
				<li>
					<CharacterLink
						id={character.id}
						firstName={character.first_name}
						lastName={character.last_name}
						cls={character.class}
					/>
					<CharacterSpecs cls={character.class} specs={character.specs} />
					{#if character.is_main}<span class="main-badge">Main</span>{/if}
					<div class="actions">
						<Button
							size="small"
							variant="secondary"
							href="{resolve('/profile/keybinds')}?character={character.id}">Keybinds</Button
						>
						{#if !character.is_main}
							<Button size="small" variant="secondary" onclick={() => makeMain(character)}>
								Make main
							</Button>
						{/if}
						<Button
							size="small"
							variant="secondary"
							onclick={() => {
								editing = character;
								open = true;
							}}
						>
							Edit
						</Button>
						{#if data.officer}
							<ConfirmButton
								size="small"
								variant="danger"
								question="Remove {character.first_name} {character.last_name}?"
								confirmLabel="Remove"
								onconfirm={() => remove(character)}>Remove</ConfirmButton
							>
						{/if}
					</div>
				</li>
			{:else}
				{#if !open}
					<li class="muted">
						{data.officer ? 'No characters yet. Start with your main.' : 'No characters yet.'}
					</li>
				{/if}
			{/each}
		</ul>

		{#if open}
			{#key editing?.id}
				<CharacterForm
					character={editing}
					firstIsMain={characters.length === 0}
					onsaved={saved}
					oncancel={() => (open = false)}
				/>
			{/key}
		{:else if data.officer}
			<div>
				<Button
					variant="primary"
					onclick={() => {
						editing = null;
						open = true;
					}}
				>
					{characters.length === 0 ? 'Add your main' : 'Add a character'}
				</Button>
			</div>
		{/if}
	</section>{/if}

<style>
	.accounts,
	.characters {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		margin: 0;
		padding: 0;
		list-style: none;
		max-width: 40rem;
	}

	.accounts li,
	.characters li {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-2) var(--space-3);
		padding: var(--space-3) var(--space-4);
		background-color: var(--grey-bg);
		border: 1px solid var(--grey-surface);
		border-radius: var(--radius-lg);
	}

	.accounts li > div {
		display: flex;
		flex: 1;
		flex-direction: column;
	}

	.accounts li.muted,
	.characters li.muted {
		background: none;
		border-style: dashed;
	}

	.identity {
		font-family: var(--font-mono);
		font-weight: 600;
	}

	.small {
		font-size: var(--text-sm);
	}

	.link-buttons {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2);
	}

	.main-badge {
		padding: 0 var(--space-2);
		color: var(--red-text);
		font-size: var(--text-xs);
		font-weight: 600;
		letter-spacing: 0.06em;
		text-transform: uppercase;
		background-color: var(--red-bg);
		border: 1px solid var(--red-soft);
		border-radius: var(--radius-md);
	}

	.actions {
		display: flex;
		gap: var(--space-2);
		margin-left: auto;
	}
</style>
