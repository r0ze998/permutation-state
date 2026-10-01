# People: players, leaders and crowds

The chain keeps no names. Each person is derived from the owner's `citizen_tag`, which is the first 8 bytes of the Citizen address (contract §4.1). Humans and shades go through the same function and look the same.

| Module | What it gives |
|---|---|
| `identity.mjs` | `identityOf(tag)` returns `{given, house, face}`. `displayName(id, {full, language})` gives `Kaito` or `Kaito Saford` / `カイト・サフォード`. `IDENTITY_VERSION` freezes the tables. |
| `avatar.mjs` | `avatarSvg(id, faction, {size})` returns markup. `avatarImage(id, faction, onLoad)` returns an `<img>` for canvases. Also `sigilPath` and the faction colours. |
| `leaders.mjs` | `LEADERS` (six names and titles), `DOCTRINE_PITCH`, and `leaderSvg(faction, {size})`. |
| `roster.mjs` | `createRoster({base})`, with `.ensure(ring)` and `.ownerOf(p, q, site, bell?)`. It reads the herald's `/h/roster/{ring}/latest.bin`; `frontier-node/crates/herald/src/roster.rs` defines the format. Without the file, no names are shown. |
| `scene.mjs` | `departuresAt(chronicle, overviews, bell)` gives the origin and arrival bell only. Also `exploresAt`, `namer(roster, {bell})` and `seedOwn(roster, holdings)`. |
| `crowds.mjs` | `paintPeople(ctx, {tiles, zoom, t, departures, explores, columnLabel})` and `paintNameTags(ctx, {tiles, zoom, nameOf, centre})`. Both work in world coordinates. |
| `ui.mjs` | `personChip`, `leaderCard`, `highlights(chronicle, overviews, roster)` and `renderHighlights`. |
| `profile.mjs` | The optional name the player sets, signed by the wallet and kept off the chain (`makeProfile` / `verifyProfile`). Where profiles are stored is an M2 decision. |

## Rules

- A sealed march shows only that it **left** and its **arrival bell**. Its column circles its own tile, with no line, no arrow and no heading. `departuresAt` never returns a destination.
- Level of detail, measured in screen pixels of hex radius:
  - Under 22: no figures.
  - 22–34: one resident per holding.
  - 34 and above: every resident and carrier.
  - Name tags: cities and strongholds from 30, every holding from 52, at most 60 per frame, nearest the view centre first.
  - At most 900 figures per frame.
- Spectators fetch one roster file per ring at most once a minute. The herald caches it per fold version, and nothing else reaches the server.

## Using it from another page (replay)

The map draws people itself when its source has `people()`:

```js
import { createRoster } from './people/roster.mjs';
import * as scene from './people/scene.mjs';
const roster = createRoster({ base });            // roster.ensure(d) for each open ring
source.people = () => ({
  departures: scene.departuresAt(records, overviews, bell),
  explores: scene.exploresAt(records, overviews, bell),
  nameOf: scene.namer(roster, { bell }),          // only owners founded by `bell`
  columnLabel: d => `Arrives at bell ${d.arriveBell}`,
});
```

A canvas overlay that draws its own scene can call `paintPeople` and `paintNameTags` directly instead, with tiles of the form `{x, y, p, pq, idx, site, state, owner, tier, fog}`.
