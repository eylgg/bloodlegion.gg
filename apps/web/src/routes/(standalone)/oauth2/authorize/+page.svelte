<script lang="ts">
	import type { PageProps } from './$types';
	import { api, ApiError } from '$lib/api';
	import Card from '$lib/components/Card.svelte';
	import Checkbox from '$lib/components/Checkbox.svelte';
	import Button from '$lib/components/Button.svelte';
	import Alert from '$lib/components/Alert.svelte';
	import FormActions from '$lib/components/FormActions.svelte';

	let { data }: PageProps = $props();

	function defaultGranted(): Record<string, boolean> {
		const result: Record<string, boolean> = {};
		for (const scope of data.scopes) {
			result[scope.name] = true;
		}
		return result;
	}

	let submitting = $state(false);
	let errorMessage = $state('');
	let granted = $state(defaultGranted());

	async function decide(approved: boolean) {
		errorMessage = '';
		const scopes = data.scopes.filter((scope) => granted[scope.name]).map((scope) => scope.name);
		submitting = true;
		try {
			const { url } = await api.post<{ url: string }>(`/api/auth/oauth2/authorize${data.query}`, {
				approved,
				scopes: approved ? scopes : []
			});
			window.location.assign(url);
			return; // navigating away; keep the buttons disabled
		} catch (err) {
			if (err instanceof ApiError && err.status >= 500) {
				errorMessage = 'Internal server error. Please try again later.';
			} else if (err instanceof ApiError && err.status > 0) {
				errorMessage = err.problem.detail || `Authorization failed (${err.status}).`;
			} else {
				errorMessage = 'Unable to reach the server. Please check your connection.';
			}
			submitting = false;
		}
	}
</script>

<svelte:head>
	<title>Authorize access | Blood Legion</title>
</svelte:head>

<Card>
	<hgroup>
		<h1>Authorize access</h1>
		<p class="subtitle">
			<strong>{data.clientName}</strong> is requesting access to your Blood Legion account.
		</p>
	</hgroup>

	{#if errorMessage}
		<Alert variant="error">{errorMessage}</Alert>
	{/if}

	<div class="scopes-block">
		<p>This will allow it to:</p>
		<ul class="scopes">
			{#each data.scopes as scope (scope.name)}
				<li>
					<Checkbox bind:checked={granted[scope.name]} disabled={submitting}>
						{scope.description}
					</Checkbox>
				</li>
			{/each}
		</ul>
	</div>

	<FormActions align="end">
		<Button type="button" variant="secondary" onclick={() => decide(false)} disabled={submitting}>
			Deny
		</Button>
		<Button type="button" variant="primary" onclick={() => decide(true)} disabled={submitting}>
			Allow
		</Button>
	</FormActions>
</Card>

<style>
	hgroup {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
	}

	.subtitle {
		color: var(--grey-text);
		font-size: var(--text-md);
	}

	.scopes-block {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}

	.scopes {
		list-style: none;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}
</style>
