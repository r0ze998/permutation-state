// The world map instance and the tile lookups that need it.
import { WorldMap, key } from './map.mjs';
import { $ } from './util.mjs';
import { S } from './state.mjs';

export { key };
export const map = new WorldMap($('#world-map'));

/** Tile key of anything with q, r (null for nothing). */
export const keyOf = x => (x ? key(x.q, x.r) : null);
export const tileIndex = id => map.tiles.get(id)?.index;
/** Owner civ of a tile as this nation knows it, or null. */
export const ownerOf = id => { const c = S.view.owners[tileIndex(id)]; return c && c !== '.' ? parseInt(c, 36) : null; };
/** '0' never seen · '1' remembered · '2' in sight. */
export const fogOf = id => S.view?.fog?.[tileIndex(id)] ?? '2';
export const unitsAt = id => S.view.units.filter(u => key(u.q, u.r) === id);
