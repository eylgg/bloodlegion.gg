<script lang="ts">
	import { CATEGORIES } from '$lib/wow/bindings';

	/** The game's Key Bindings panel: every command with its two keys, by category. */
	let {
		bindings,
		capturing,
		oncapture
	}: {
		bindings: Record<string, [string, string]>;
		capturing: { command: string; index: number } | null;
		oncapture: (command: string, index: number) => void;
	} = $props();
</script>

<div class="panel">
	{#each CATEGORIES as category, i (category.name)}
		<details open={i === 0}>
			<summary>{category.name}</summary>
			<table>
				<thead>
					<tr><th>Command</th><th>Key 1</th><th>Key 2</th></tr>
				</thead>
				<tbody>
					{#each category.commands as { command, label } (command)}
						<tr>
							<td>{label}</td>
							{#each [0, 1] as index (index)}
								{@const key = bindings[command]?.[index] ?? ''}
								{@const active = capturing?.command === command && capturing.index === index}
								<td>
									<button
										type="button"
										class:active
										title={key || 'Not bound'}
										aria-label="{label}, key {index + 1}: {key || 'not bound'}"
										onclick={() => oncapture(command, index)}
										>{active ? 'Press a key…' : key}</button
									>
								</td>
							{/each}
						</tr>
					{/each}
				</tbody>
			</table>
		</details>
	{/each}
</div>

<style>
	.panel {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}

	details {
		background: var(--grey-bg);
		border: 1px solid var(--grey-surface);
		border-radius: var(--radius-lg);
	}

	summary {
		padding: var(--space-2) var(--space-3);
		font-weight: 600;
		cursor: pointer;
	}

	table {
		width: 100%;
		border-collapse: collapse;
		font-size: var(--text-sm);
	}

	th {
		padding: var(--space-1) var(--space-3);
		font-size: var(--text-xs);
		font-weight: 600;
		text-align: left;
		color: var(--grey-text);
	}

	td {
		padding: 1px var(--space-3);
		border-top: 1px solid var(--grey-surface);
	}

	td:not(:first-child) {
		width: 9rem;
	}

	td button {
		width: 100%;
		min-height: 1.6rem;
		padding: 0 var(--space-2);
		font: inherit;
		font-family: var(--font-mono);
		font-size: var(--text-xs);
		text-align: left;
		background: var(--black);
		border: 1px solid var(--grey-surface);
		border-radius: var(--radius-sm);
		cursor: pointer;
	}

	td button:hover {
		border-color: var(--grey-text);
	}

	td button.active {
		color: var(--accent-text);
		border-color: var(--accent-solid);
	}
</style>
