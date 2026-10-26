<script lang="ts">
	import { api, errorMessage } from '$lib/api';
	import Button from '$lib/components/Button.svelte';
	import Alert from '$lib/components/Alert.svelte';
	import type { DiscordSettings } from '$lib/types';

	let { settings: initial }: { settings: DiscordSettings } = $props();

	// svelte-ignore state_referenced_locally
	let settings = $state(initial);
	// svelte-ignore state_referenced_locally
	let guildId = $state(initial.guild_id ?? '');
	// The token is never sent back: the field is only for a new one.
	let token = $state('');
	let saving = $state(false);
	let error = $state('');
	let saved = $state('');

	async function put(
		change: { guild_id?: string | null; bot_token?: string | null },
		done: string
	) {
		saving = true;
		error = '';
		saved = '';
		try {
			settings = await api.put<DiscordSettings>('/api/discord/settings', change);
			guildId = settings.guild_id ?? '';
			token = '';
			saved = done;
		} catch (err) {
			error = errorMessage(err, 'Saving failed. Please try again.');
		} finally {
			saving = false;
		}
	}

	function save(event: SubmitEvent) {
		event.preventDefault();
		const change: { guild_id: string | null; bot_token?: string } = {
			guild_id: guildId.trim() || null
		};
		if (token.trim()) change.bot_token = token.trim();
		put(change, 'Saved. The bot restarts with the new settings.');
	}

	function removeToken() {
		if (!confirm('Remove the bot token? The bot stops until a new one is set.')) return;
		put({ bot_token: null }, 'Token removed. The bot has stopped.');
	}
</script>

<form class="panel" onsubmit={save}>
	<h2>Discord bot</h2>
	<p class="muted">
		The bot keeps the guild's Discord server in step with the site: roles that follow ranks,
		categories that follow access, and #announcements on the front page. It runs once both are set.
		The token is stored encrypted and never shown again.
	</p>
	{#if error}<Alert variant="error">{error}</Alert>{/if}
	{#if saved}<Alert variant="success">{saved}</Alert>{/if}
	<div class="row">
		<label class="field">
			Server ID
			<input
				bind:value={guildId}
				inputmode="numeric"
				autocomplete="off"
				placeholder="Right-click the server, Copy Server ID"
			/>
		</label>
		<label class="field">
			Bot token
			<input
				type="password"
				bind:value={token}
				autocomplete="off"
				placeholder={settings.has_bot_token ? 'Set (enter a new one to replace it)' : 'Not set'}
			/>
		</label>
	</div>
	<p class="muted small">
		{#if settings.guild_id && settings.has_bot_token}
			Both are set, so the bot is running (or starting).
		{:else}
			The bot is not running: it needs {settings.guild_id
				? 'a token'
				: settings.has_bot_token
					? 'a server ID'
					: 'a server ID and a token'}.
		{/if}
	</p>
	<div class="form-actions">
		{#if settings.has_bot_token}
			<Button variant="danger" disabled={saving} onclick={removeToken}>Remove token</Button>
		{/if}
		<Button type="submit" variant="primary" disabled={saving}>
			{saving ? 'Checking...' : 'Save'}
		</Button>
	</div>
</form>

<style>
	.panel {
		max-width: 44rem;
	}

	.small {
		font-size: var(--text-sm);
	}
</style>
