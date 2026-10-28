/**
 * A question asked in the page rather than in a browser dialog, for an action that is already
 * under way (a drop, say) and so has no button to swap out. `ask` shows `question` until `answer`
 * is called, then resolves to the answer; asking again first answers the open question no.
 * Render it with `AskBar`.
 */
export class Ask {
	question = $state<string | null>(null);
	#resolve: ((yes: boolean) => void) | null = null;

	ask = (question: string) => {
		this.answer(false);
		this.question = question;
		return new Promise<boolean>((resolve) => (this.#resolve = resolve));
	};

	answer = (yes: boolean) => {
		const resolve = this.#resolve;
		this.#resolve = null;
		this.question = null;
		resolve?.(yes);
	};
}
