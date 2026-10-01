<script lang="ts">
	import type { PageProps } from './$types';
	import { api, errorMessage } from '$lib/api';
	import Button from '$lib/components/Button.svelte';
	import Card from '$lib/components/Card.svelte';
	import Input from '$lib/components/Input.svelte';
	import Alert from '$lib/components/Alert.svelte';
	import FormActions from '$lib/components/FormActions.svelte';

	let { data }: PageProps = $props();

	// The suggestion seeds the field once; from then on the field is the person's own.
	// svelte-ignore state_referenced_locally
	let username = $state(data.registration.suggestion ?? '');
	let error = $state('');
	let submitting = $state(false);

	async function choose(event: SubmitEvent) {
		event.preventDefault();
		submitting = true;
		error = '';
		try {
			const { url } = await api.post<{ url: string }>('/api/auth/oauth2/registration', {
				username
			});
			window.location.assign(url);
			return;
		} catch (err) {
			error = errorMessage(err, 'Something went wrong. Please try again.');
			submitting = false;
		}
	}
</script>

<svelte:head>
	<title>Choose a username | Blood Legion</title>
</svelte:head>

<Card>
	<hgroup>
		<h1>Choose a username</h1>
		<p class="subtitle">
			You're signing in with {data.registration.provider_name}{#if data.registration.identity}
				as <strong>{data.registration.identity}</strong>{/if}. Pick the name you'll go by here.
		</p>
	</hgroup>

	{#if error}
		<Alert variant="error">{error}</Alert>
	{/if}

	<form class="register-form" onsubmit={choose}>
		<Input
			label="Username"
			name="username"
			type="text"
			required
			minlength={3}
			maxlength={32}
			pattern="[A-Za-z][A-Za-z0-9]*"
			autocomplete="username"
			autocapitalize="off"
			spellcheck={false}
			bind:value={username}
		/>
		<p class="hint">
			3 to 32 characters: a letter, then letters and digits. Capitals are kept for display; names
			are unique regardless of case.
		</p>
		<FormActions align="end">
			<Button type="submit" variant="primary" disabled={submitting}>
				{submitting ? 'Creating account...' : 'Continue'}
			</Button>
		</FormActions>
	</form>
</Card>

<style>
	hgroup {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
	}

	.subtitle,
	.hint {
		color: var(--grey-text);
		font-size: var(--text-md);
	}

	.register-form {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
	}
</style>
