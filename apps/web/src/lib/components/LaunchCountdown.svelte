<script lang="ts">
	/**
	 * The countdown to WoW: Forever's global launch: November 4, 2026 at 23:00 UTC (3:00 PM
	 * Pacific, 6:00 PM Eastern), one moment worldwide. The time is shown in the visitor's own zone.
	 * It ticks only in the browser: rendered on the server it would be stale by the time it arrived.
	 */
	const LAUNCH = Date.UTC(2026, 10, 4, 23, 0, 0);

	let now = $state<number | null>(null);

	$effect(() => {
		now = Date.now();
		const timer = setInterval(() => (now = Date.now()), 1000);
		return () => clearInterval(timer);
	});

	const remaining = $derived(now === null ? null : Math.max(0, LAUNCH - now));
	const parts = $derived.by(() => {
		if (remaining === null) return null;
		const seconds = Math.floor(remaining / 1000);
		return [
			{ value: Math.floor(seconds / 86400), unit: 'days' },
			{ value: Math.floor((seconds % 86400) / 3600), unit: 'hours' },
			{ value: Math.floor((seconds % 3600) / 60), unit: 'minutes' },
			{ value: seconds % 60, unit: 'seconds' }
		];
	});

	const local = $derived(
		now === null
			? 'November 4, 2026 at 23:00 UTC'
			: new Intl.DateTimeFormat(undefined, {
					weekday: 'long',
					month: 'long',
					day: 'numeric',
					hour: 'numeric',
					minute: '2-digit',
					timeZoneName: 'short'
				}).format(LAUNCH)
	);
</script>

<div class="countdown" aria-live="off">
	{#if remaining === 0}
		<p class="live">World of Warcraft: Forever is live.</p>
	{:else}
		<ol class="units" aria-label="Time until launch">
			{#each parts ?? [{ value: null, unit: 'days' }, { value: null, unit: 'hours' }, { value: null, unit: 'minutes' }, { value: null, unit: 'seconds' }] as part (part.unit)}
				<li>
					<span class="value"
						>{part.value === null ? '--' : String(part.value).padStart(2, '0')}</span
					>
					<span class="unit">{part.unit}</span>
				</li>
			{/each}
		</ol>
	{/if}
	<p class="when">Launches worldwide {local}</p>
</div>

<style>
	.countdown {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}

	.units {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		gap: var(--space-2);
	}

	.units li {
		display: flex;
		flex-direction: column;
		align-items: center;
		min-width: 4.5rem;
		padding: var(--space-2) var(--space-3);
		background: linear-gradient(
			180deg,
			var(--red-bg),
			color-mix(in oklch, var(--red-bg) 40%, var(--background))
		);
		border: 1px solid var(--red-soft);
		border-radius: var(--radius-lg);
	}

	.value {
		font-family: var(--font-mono);
		font-size: clamp(1.5rem, 5vw, 2.25rem);
		font-weight: 700;
		line-height: 1.1;
		font-variant-numeric: tabular-nums;
	}

	.unit {
		color: var(--grey-text);
		font-size: var(--text-xs);
		text-transform: uppercase;
		letter-spacing: 0.08em;
	}

	.when,
	.live {
		color: var(--grey-text-active);
		font-size: var(--text-md);
	}

	.live {
		font-size: var(--text-xl);
		font-weight: 700;
		color: var(--red-text);
	}

	@media (max-width: 26rem) {
		.units li {
			min-width: 0;
			flex: 1;
		}
	}
</style>
