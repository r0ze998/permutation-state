// The poll entry point, kept apart from app.mjs so that modules which only
// need "refresh now" (input, orders, lobby, drawers) do not import app.mjs,
// which imports them back. app.mjs registers the real fetch-and-apply step
// at load time, before anything can call poll().
import { singleFlight } from './util.mjs';

let pollStep = async () => {};

/** app.mjs: the one fetch-and-apply step that poll() runs. */
export function setPollStep(fn) { pollStep = fn; }

/** Fetch and apply the latest view; overlapping calls are coalesced (one request in flight). */
export const poll = singleFlight(() => pollStep());
