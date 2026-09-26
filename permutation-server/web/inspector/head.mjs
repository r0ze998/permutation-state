// Shared inspector heading.
import { html } from '../util.mjs';
import { L } from '../lang.mjs';

/** Panel heading: close button, eyebrow, title (text or html) and an optional description. */
export const head = (eyebrow, title, desc = '') => html`<button class="close-x" type="button" data-close aria-label="${L`閉じる`}">×</button><div class="eyebrow">${eyebrow}</div><h2>${title}</h2>${desc ? html`<p class="desc">${desc}</p>` : ''}`;
