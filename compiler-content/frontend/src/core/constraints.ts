// =========================================================================
//  ConstraintRegistry — extensible line-level constraint system
// =========================================================================

import type { ConstraintHandler, Constraints, EditorConfig } from './types';

type HandlerMap = Record<string, ConstraintHandler>;

const handlers: HandlerMap = {};

/**
 * Register a constraint handler. Call before init.
 * Built-in handlers: readonly, hidden, suggested.
 */
export function registerConstraint(name: string, handler: ConstraintHandler): void {
  handlers[name] = handler;
}

/**
 * Translate the backend's **region**-style constraints into the line-number
 * lists this editor works with.
 *
 * The Rust side stores `readonlyPrefix` / `readonlySuffix` ("first N lines",
 * "last N lines") precisely so that a player inserting lines in the middle
 * cannot break the lock. Here we expand the prefix to absolute line numbers and
 * keep the suffix as negative numbers ("-1" = last line) because at this point
 * we do not know how many lines the program has.
 */
export function toEditorConstraints(config: EditorConfig): Constraints {
  const c = config.constraints ?? {};
  const readonly: number[] = [...(c.readonly ?? [])];

  const prefix = c.readonlyPrefix ?? 0;
  for (let i = 1; i <= prefix; i++) readonly.push(i);

  const suffix = c.readonlySuffix ?? 0;
  for (let i = 1; i <= suffix; i++) readonly.push(-i);

  return {
    readonly,
    hidden: c.hidden ?? [],
    suggested: c.suggested ?? {},
    highlight: c.highlight ?? [],
  };
}

/**
 * Apply all constraints from config to the editor line elements.
 *
 * A line number may be negative: `-1` means the last line, `-2` the one before
 * it. Out-of-range numbers are ignored (a level page should not be able to
 * crash the editor by naming line 900 of a 5-line program).
 */
export function applyConstraints(
  constraints: Constraints,
  lineEls: HTMLTextAreaElement[],
  config: EditorConfig,
): void {
  const total = lineEls.length;

  /** Resolve a 1-based line number (negative = from the end) into an index. */
  const toIndex = (n: number): number => {
    const i = n < 0 ? total + n : n - 1;
    return i >= 0 && i < total ? i : -1;
  };

  (constraints.readonly ?? []).forEach((n) => {
    const i = toIndex(n);
    if (i >= 0 && handlers.readonly) handlers.readonly(lineEls[i], i, true, config);
  });

  (constraints.hidden ?? []).forEach((n) => {
    const i = toIndex(n);
    if (i >= 0 && handlers.hidden) handlers.hidden(lineEls[i], i, true, config);
  });

  (constraints.highlight ?? []).forEach((n) => {
    const i = toIndex(n);
    if (i >= 0) lineEls[i].classList.add('riscv-line--highlight');
  });

  if (constraints.suggested) {
    Object.keys(constraints.suggested).forEach((k) => {
      const n = parseInt(k, 10);
      if (Number.isNaN(n)) return;
      const i = toIndex(n);
      if (i >= 0 && handlers.suggested) {
        handlers.suggested(lineEls[i], i, constraints.suggested![k], config);
      }
    });
  }
}

// ── Built-in handlers ──────────────────────────────────────────────────

registerConstraint('readonly', (lineEl: HTMLTextAreaElement) => {
  lineEl.readOnly = true;
  lineEl.classList.add('riscv-line--readonly');
  lineEl.setAttribute('tabindex', '-1');
});

registerConstraint('hidden', (lineEl: HTMLTextAreaElement) => {
  lineEl.style.display = 'none';
  lineEl.classList.add('riscv-line--hidden');
  // Also hide the corresponding gutter line
  const n = lineEl.getAttribute('data-line');
  const gl = document.querySelector(`.riscv-gutter-line[data-line="${n}"]`);
  if (gl) (gl as HTMLElement).style.display = 'none';
});

registerConstraint('suggested', (lineEl: HTMLTextAreaElement, _idx: number, text: unknown) => {
  if (text && typeof text === 'string') {
    lineEl.setAttribute('placeholder', text);
  }
});
