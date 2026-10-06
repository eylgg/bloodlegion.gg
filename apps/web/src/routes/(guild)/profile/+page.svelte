<script lang="ts">
	import { page } from '$app/state';
	import { api, errorMessage } from '$lib/api';
	import { RANK_LABEL, formatDate } from '$lib/guild';
	import Button from '$lib/components/Button.svelte';
	import Alert from '$lib/components/Alert.svelte';
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

	// A link comes back as `/profile?linked=new|existing` or `/profile?error=<code>` (see the
	// backend's `links` module and `ProviderError::code`). Known codes only, so a crafted link
	// cannot put arbitrary words on the page.
	const LINK_ERRORS: Record<string, string> = {
		already_linked:
			'That Battle.net account is already linked to another Blood Legion account. Unlink it ' +
			'there first.',
		permissions_required:
			'Linking needs every permission the site asks for, and Battle.net remembers what you ' +
			'allowed before. Remove Blood Legion from the applications connected to that account at ' +
			'account.battle.net, then link it again and leave every permission ticked.',
		cancelled: 'Linking was cancelled.',
		expired: 'That took too long or was already used. Please try again.',
		verification_failed: 'We could not verify that account. Please try again.',
		unknown_provider: 'That sign-in method is not available.',
		unavailable: 'Linking is unavailable right now. Please try again later.'
	};
	const linked = $derived(page.url.searchParams.get('linked'));
	const linkError = $derived.by(() => {
		const code = page.url.searchParams.get('error');
		return code ? (LINK_ERRORS[code] ?? 'Linking failed. Please try again.') : null;
	});

	async function unlink(account: LinkedAccount) {
		const name = account.identity ?? `this ${account.provider_name} account`;
		if (!confirm(`Unlink ${name}? You will no longer be able to sign in with it.`)) return;
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
					is_main: true
				})
			);
		} catch (err) {
			error = errorMessage(err, 'That did not work. Please try again.');
		}
	}

	async function remove(character: GuildCharacter) {
		if (!confirm(`Remove ${character.first_name} ${character.last_name}?`)) return;
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

<section>
	<h2>Battle.net accounts</h2>
	<p class="muted">
		Link every Battle.net account you play on, and sign in with any of them. Characters will come
		from these accounts once Blizzard's API serves WoW: Forever.
	</p>

	{#if linked === 'new'}
		<Alert variant="success">Account linked.</Alert>
	{:else if linked === 'existing'}
		<Alert variant="info">
			Battle.net signed you straight back into an account that is already linked. To link a
			different one, sign out at battle.net first (or pick another account when it asks), then try
			again.
		</Alert>
	{/if}
	{#if linkError}<Alert variant="error">{linkError}</Alert>{/if}

	<ul class="accounts">
		{#each accounts as account (account.id)}
			<li>
				<div>
					<span class="identity">{account.identity ?? 'Unnamed account'}</span>
					<span class="muted">{account.provider_name} · linked {formatDate(account.linked_at)}</span
					>
				</div>
				{#if account.can_unlink}
					<Button size="small" variant="danger" onclick={() => unlink(account)}>Unlink</Button>
				{/if}
			</li>
		{:else}
			<li class="muted">No accounts linked.</li>
		{/each}
	</ul>
	{#if accounts.length > 0 && !accounts.some((a) => a.can_unlink)}
		<p class="muted small">
			{accounts.length === 1
				? 'Your only linked account is how you sign in, so it cannot be unlinked.'
				: 'Unlinking is turned off for now.'}
		</p>
	{/if}

	<div class="link-buttons">
		{#each data.providers as provider (provider.slug)}
			<!-- A full page load: the backend sends the browser on to the provider. -->
			<Button
				data-sveltekit-reload
				href={`/api/auth/oauth2/providers/${provider.slug}/link`}
				variant={provider.slug === 'battlenet' ? 'battlenet' : 'secondary'}
			>
				Link another {provider.name} account
			</Button>
		{/each}
	</div>
</section>

<section>
	<h2>Your characters</h2>
	<p class="muted">
		First and last name, as they are in game. Officers record loot and attendance against these.
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
					<Button size="small" variant="danger" onclick={() => remove(character)}>Remove</Button>
				</div>
			</li>
		{:else}
			{#if !open}<li class="muted">No characters yet. Start with your main.</li>{/if}
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
	{:else}
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
</section>

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
