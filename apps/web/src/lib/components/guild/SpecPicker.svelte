<script lang="ts">
	import RoleIcon from '$lib/components/RoleIcon.svelte';
	import { specIcon } from '$lib/wow/icons';
	import type { SpecChoice, WowClass } from '$lib/types';

	/**
	 * One of a character's two specs: the spec (or none), and the class's notable talents it takes.
	 * Picking a spec ticks the talents of its own tree, which can then be unticked, or others
	 * from other trees ticked: hybrid builds take them.
	 */
	let {
		wowClass,
		legend,
		value = $bindable(null)
	}: { wowClass: WowClass; legend: string; value?: SpecChoice | null } = $props();

	function pick(slug: string | null) {
		if (slug === null) {
			value = null;
		} else if (slug !== value?.spec) {
			value = {
				spec: slug,
				talents: wowClass.talents.filter((t) => t.tree === slug).map((t) => t.slug)
			};
		}
	}

	function toggle(talent: string) {
		if (!value) return;
		const talents = value.talents.includes(talent)
			? value.talents.filter((t) => t !== talent)
			: [...value.talents, talent];
		value = { ...value, talents };
	}
</script>

<fieldset style:--class-color={wowClass.color}>
	<legend>{legend}</legend>
	<div class="specs">
		{#each wowClass.specs as spec (spec.slug)}
			<button
				type="button"
				class="spec"
				class:selected={value?.spec === spec.slug}
				aria-pressed={value?.spec === spec.slug}
				onclick={() => pick(spec.slug)}
			>
				<img src={specIcon(wowClass.slug, spec.slug)} alt="" width="24" height="24" />
				<span>{spec.name}</span>
				<span class="roles">
					{#each spec.roles as role (role)}<RoleIcon {role} size={14} />{/each}
				</span>
			</button>
		{/each}
		<button
			type="button"
			class="spec none"
			class:selected={value === null}
			aria-pressed={value === null}
			onclick={() => pick(null)}>None</button
		>
	</div>
	{#if value && wowClass.talents.length > 0}
		<div class="talents">
			{#each wowClass.talents as talent (talent.slug)}
				<label class="talent">
					<input
						type="checkbox"
						checked={value.talents.includes(talent.slug)}
						onchange={() => toggle(talent.slug)}
					/>
					<span>{talent.name}</span>
					<span class="tree">
						{wowClass.specs.find((s) => s.slug === talent.tree)?.name ?? talent.tree}
					</span>
				</label>
			{/each}
		</div>
	{/if}
</fieldset>

<style>
	fieldset {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		margin: 0;
		padding: 0;
		border: 0;
	}

	legend {
		margin-bottom: var(--space-1);
		color: var(--grey-text);
		font-size: var(--text-sm);
		font-weight: 500;
	}

	.specs {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2);
	}

	.spec {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		padding: var(--space-1) var(--space-2) var(--space-1) var(--space-1);
		color: var(--foreground);
		font: inherit;
		font-size: var(--text-md);
		background-color: var(--background);
		border: 1px solid var(--grey-soft);
		border-radius: var(--radius-md);
		cursor: pointer;
	}

	.spec.none {
		padding: var(--space-1) var(--space-3);
		color: var(--grey-text);
	}

	.spec img {
		display: block;
		border-radius: var(--radius-sm);
	}

	.spec:hover,
	.spec.selected {
		border-color: var(--class-color);
	}

	.spec.selected {
		background-color: color-mix(in oklch, var(--class-color) 14%, var(--background));
	}

	.roles {
		display: flex;
		gap: 2px;
	}

	.talents {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(15rem, 1fr));
		gap: var(--space-1) var(--space-4);
		padding-left: var(--space-1);
	}

	.talent {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		font-size: var(--text-md);
		cursor: pointer;
	}

	.talent input {
		accent-color: var(--accent-solid);
	}

	.tree {
		color: var(--grey-text);
		font-size: var(--text-xs);
	}
</style>
