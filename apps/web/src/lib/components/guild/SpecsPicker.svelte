<script lang="ts">
	import RoleIcon from '$lib/components/RoleIcon.svelte';
	import { specIcon } from '$lib/wow/icons';
	import type { SpecsInput, WowClass } from '$lib/types';

	/**
	 * The specs a character plays: any of the class's, each with the class's notable talents it
	 * takes, and at most one main. Turning a spec on ticks the talents of its own tree, which can
	 * then be unticked, or others from other trees ticked: hybrid builds take them.
	 */
	let { wowClass, value = $bindable() }: { wowClass: WowClass; value: SpecsInput } = $props();
	const uid = $props.id();

	const played = (slug: string) => value.specs.find((s) => s.spec === slug);

	function toggleSpec(slug: string) {
		if (played(slug)) {
			value = {
				specs: value.specs.filter((s) => s.spec !== slug),
				main: value.main === slug ? null : value.main
			};
		} else {
			const talents = wowClass.talents.filter((t) => t.tree === slug).map((t) => t.slug);
			// A first spec is the main until said otherwise.
			value = {
				specs: [...value.specs, { spec: slug, talents }],
				main: value.specs.length === 0 ? slug : value.main
			};
		}
	}

	function toggleTalent(slug: string, talent: string) {
		value = {
			...value,
			specs: value.specs.map((s) =>
				s.spec !== slug
					? s
					: {
							...s,
							talents: s.talents.includes(talent)
								? s.talents.filter((t) => t !== talent)
								: [...s.talents, talent]
						}
			)
		};
	}
</script>

<fieldset style:--class-color={wowClass.color}>
	<legend>Specs played</legend>
	{#each wowClass.specs as spec (spec.slug)}
		{@const chosen = played(spec.slug)}
		<div class="spec" class:on={chosen}>
			<div class="head">
				<label class="plays">
					<input type="checkbox" checked={!!chosen} onchange={() => toggleSpec(spec.slug)} />
					<img src={specIcon(wowClass.slug, spec.slug)} alt="" width="24" height="24" />
					<span>{spec.name}</span>
					<span class="roles">
						{#each spec.roles as role (role)}<RoleIcon {role} size={14} />{/each}
					</span>
				</label>
				{#if chosen}
					<label class="main">
						<input
							type="radio"
							name="{uid}-main"
							checked={value.main === spec.slug}
							onchange={() => (value = { ...value, main: spec.slug })}
						/>
						Main spec
					</label>
				{/if}
			</div>
			{#if chosen && wowClass.talents.length > 0}
				<div class="talents">
					{#each wowClass.talents as talent (talent.slug)}
						<label class="talent">
							<input
								type="checkbox"
								checked={chosen.talents.includes(talent.slug)}
								onchange={() => toggleTalent(spec.slug, talent.slug)}
							/>
							<span>{talent.name}</span>
							<span class="tree">
								{wowClass.specs.find((s) => s.slug === talent.tree)?.name ?? talent.tree}
							</span>
						</label>
					{/each}
				</div>
			{/if}
		</div>
	{/each}
	{#if value.specs.length > 0}
		<label class="main no-main">
			<input
				type="radio"
				name="{uid}-main"
				checked={value.main === null}
				onchange={() => (value = { ...value, main: null })}
			/>
			No main spec
		</label>
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

	.spec {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		padding: var(--space-2) var(--space-3);
		background-color: var(--background);
		border: 1px solid var(--grey-soft);
		border-radius: var(--radius-md);
	}

	.spec.on {
		border-color: var(--class-color);
		background-color: color-mix(in oklch, var(--class-color) 8%, var(--background));
	}

	.head {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-2);
	}

	.plays,
	.main,
	.talent {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		font-size: var(--text-md);
		cursor: pointer;
	}

	.plays span:first-of-type {
		font-weight: 600;
	}

	.plays img {
		display: block;
		border-radius: var(--radius-sm);
	}

	.roles {
		display: flex;
		gap: 2px;
	}

	.main {
		color: var(--grey-text);
		font-size: var(--text-sm);
	}

	.no-main {
		padding-left: var(--space-3);
	}

	input {
		accent-color: var(--accent-solid);
	}

	.talents {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(15rem, 1fr));
		gap: var(--space-1) var(--space-4);
		padding-left: var(--space-6);
	}

	.tree {
		color: var(--grey-text);
		font-size: var(--text-xs);
	}
</style>
