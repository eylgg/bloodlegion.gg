<script lang="ts">
	import type { HTMLInputAttributes } from 'svelte/elements';

	// This component's `value` is a string. Svelte's `bind:value` coerces to a
	// `number` on `type="number"`/`"range"` (and would break every caller that does
	// string work on the value), so the type prop is restricted to the string-valued
	// input types. For numeric entry, use `inputmode="numeric"` on a text input and
	// parse on submit.
	type StringInputType =
		| 'text'
		| 'email'
		| 'password'
		| 'url'
		| 'tel'
		| 'search'
		| 'date'
		| 'datetime-local'
		| 'month'
		| 'week'
		| 'time'
		| 'color'
		| 'hidden';

	type Props = Omit<HTMLInputAttributes, 'type'> & {
		label?: string;
		value?: string;
		type?: StringInputType;
	};

	const uid = $props.id();

	let { label, id = uid, type = 'text', value = $bindable(''), ...rest }: Props = $props();
</script>

<div class="field">
	{#if label}
		<label for={id}>{label}</label>
	{/if}
	<input {id} {type} bind:value {...rest} />
</div>

<style>
	.field {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
	}

	label {
		font-size: var(--text-sm);
		font-weight: 500;
		color: var(--grey-text);
	}

	input {
		width: 100%;
		padding: var(--space-2) var(--space-3);
		color: var(--foreground);
		background-color: var(--background);
		border: 1px solid var(--grey-soft);
		border-radius: var(--radius-md);
		transition:
			border-color 0.15s ease,
			box-shadow 0.15s ease;
	}

	input:focus {
		outline: none;
		border-color: var(--accent-solid);
		box-shadow: 0 0 0 3px var(--accent-bg);
	}

	input:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}
</style>
