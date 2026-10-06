<script lang="ts">
	import type { HTMLAnchorAttributes, HTMLButtonAttributes } from 'svelte/elements';
	import type { Snippet } from 'svelte';

	// `battlenet` is the one provider-branded variant: Battle.net's blue, for its sign-in button.
	type ButtonVariant = 'primary' | 'secondary' | 'danger' | 'battlenet';
	type ButtonSize = 'default' | 'small';

	type AnchorProps = HTMLAnchorAttributes & {
		href: string;
		variant?: ButtonVariant;
		size?: ButtonSize;
		full?: boolean;
		children?: Snippet;
		type?: never;
	};

	type NativeButtonProps = HTMLButtonAttributes & {
		href?: never;
		variant?: ButtonVariant;
		size?: ButtonSize;
		full?: boolean;
		children?: Snippet;
		type?: 'button' | 'submit' | 'reset';
	};

	type Props = AnchorProps | NativeButtonProps;

	let {
		href,
		variant = 'primary',
		size = 'default',
		full = false,
		type = href ? undefined : 'button',
		children,
		...restProps
	}: Props = $props();

	const classes = $derived(
		`btn btn-${variant}${size === 'small' ? ' btn-small' : ''}${full ? ' btn-full' : ''}`
	);
</script>

{#if href}
	<!-- Button is a generic anchor: the caller supplies the href (which may be an
	     external URL or an already-resolved app route), so resolution is the
	     caller's responsibility, not this component's. -->
	<!-- eslint-disable svelte/no-navigation-without-resolve -->
	<a {href} class={classes} {...restProps as HTMLAnchorAttributes}>
		{@render children?.()}
	</a>
	<!-- eslint-enable svelte/no-navigation-without-resolve -->
{:else}
	<button {type} class={classes} {...restProps as HTMLButtonAttributes}>
		{@render children?.()}
	</button>
{/if}

<style>
	.btn {
		/* Set, not left to the browser: iOS Safari colors button text its own system blue. */
		color: var(--foreground);
		display: inline-flex;
		align-items: center;
		justify-content: center;
		gap: var(--space-2);

		font-size: var(--text-sm);
		font-weight: 500;
		text-decoration: none;

		padding: var(--space-2) var(--space-4);
		border-radius: var(--radius-md);
		border: 1px solid transparent;
		cursor: pointer;
		transition: all 0.15s ease;
	}

	/* Compact variant for dense contexts like table row actions. */
	.btn-small {
		gap: var(--space-1);
		padding: var(--space-1) var(--space-2);
		font-size: var(--text-xs);
	}

	/* Opt-in full width, e.g. a form's primary call-to-action. Without this a
	   button placed in a flex-column form would still stretch by default; making
	   it explicit keeps width a deliberate choice rather than a layout accident. */
	.btn-full {
		width: 100%;
	}

	.btn:disabled,
	.btn[aria-disabled='true'] {
		opacity: 0.5;
		cursor: not-allowed;
		pointer-events: none;
	}

	.btn-primary {
		background-color: var(--accent-soft);
		&:hover {
			background-color: var(--accent-solid);
		}
	}

	.btn-secondary {
		background-color: var(--grey-soft);
		&:hover {
			background-color: var(--grey-solid);
		}
	}

	.btn-danger {
		background-color: var(--red-soft);
		&:hover {
			background-color: var(--red-solid);
		}
	}

	.btn-battlenet {
		color: #fff;
		background-color: #148eff;
		&:hover {
			background-color: #0b7be6;
		}
	}
</style>
