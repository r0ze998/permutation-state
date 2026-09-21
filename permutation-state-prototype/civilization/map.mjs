/** A state-driven, dependency-free hex diorama. No decorative game entities are invented. */
const SQRT3 = Math.sqrt(3);
const RADIUS = 47;
const FLATTEN = 0.76;
const LABELS = {
  townhall: '共同倉庫', farm: '農場', lumbermill: '製材所', quarry: '採石場',
  mine: '鉱山', workshop: '工房', watchtower: '見張り塔', archive: '学術院',
};
const COLORS = {
  grass: ['#c6cea0', '#a1ad80', '#899a71'],
  forest: ['#99b08a', '#748e70', '#61765f'],
  hill: ['#c5b897', '#a79b80', '#8b816b'],
  mountain: ['#afa99c', '#928e83', '#7c7b71'],
  water: ['#77a6a6', '#558a91', '#467880'],
  fog: ['#d2d6cb', '#b7c1b7', '#a5b3aa'],
};
const NEIGHBORS = [[1, 0], [1, -1], [0, -1], [-1, 0], [-1, 1], [0, 1]];
const clamp = (v, lo, hi) => Math.max(lo, Math.min(hi, v));
const key = (q, r) => `${q},${r}`;
const project = (q, r) => ({ x: SQRT3 * RADIUS * (q + r / 2), y: RADIUS * 1.5 * r * FLATTEN });
const seed = (q, r, n = 0) => {
  const v = Math.sin(q * 127.1 + r * 311.7 + n * 74.7) * 43758.5453;
  return v - Math.floor(v);
};
function rounded(ctx, x, y, w, h, r = 6) {
  ctx.beginPath();
  ctx.moveTo(x + r, y); ctx.lineTo(x + w - r, y);
  ctx.quadraticCurveTo(x + w, y, x + w, y + r);
  ctx.lineTo(x + w, y + h - r); ctx.quadraticCurveTo(x + w, y + h, x + w - r, y + h);
  ctx.lineTo(x + r, y + h); ctx.quadraticCurveTo(x, y + h, x, y + h - r);
  ctx.lineTo(x, y + r); ctx.quadraticCurveTo(x, y, x + r, y); ctx.closePath();
}
function polygon(ctx, points, fill, stroke, width = 1) {
  ctx.beginPath(); points.forEach(([x, y], i) => i ? ctx.lineTo(x, y) : ctx.moveTo(x, y));
  ctx.closePath(); if (fill) { ctx.fillStyle = fill; ctx.fill(); }
  if (stroke) { ctx.strokeStyle = stroke; ctx.lineWidth = width; ctx.stroke(); }
}
function hexPoints(x, y, inset = 0) {
  return Array.from({ length: 6 }, (_, i) => {
    const a = (i * 60 - 30) * Math.PI / 180;
    return [x + Math.cos(a) * (RADIUS - inset), y + Math.sin(a) * (RADIUS - inset) * FLATTEN];
  });
}
function inverseHex(x, y) {
  const r = (2 / 3 * (y / FLATTEN)) / RADIUS;
  const q = (SQRT3 / 3 * x - (y / FLATTEN) / 3) / RADIUS;
  let aq = Math.round(q), ar = Math.round(r), as = Math.round(-q - r);
  const dq = Math.abs(aq - q), dr = Math.abs(ar - r), ds = Math.abs(as + q + r);
  if (dq > dr && dq > ds) aq = -ar - as;
  else if (dr > ds) ar = -aq - as;
  return key(aq, ar);
}

export class CivilizationMap {
  constructor(canvas, { onSelect = () => {}, onHover = () => {}, onMove = () => {} } = {}) {
    this.canvas = canvas;
    this.ctx = canvas.getContext('2d', { alpha: false });
    this.callbacks = { onSelect, onHover, onMove };
    this.world = null;
    this.actorId = null;
    this.tiles = new Map();
    this.selection = null;
    this.hover = null;
    this.lens = 'normal';
    this.zoom = 1.18;
    this.offset = { x: 0, y: 0 };
    this.targetOffset = null;
    this.positions = new Map();
    this.cache = document.createElement('canvas');
    this.cacheCtx = this.cache.getContext('2d', { alpha: false });
    this.dirty = true;
    this.destroyed = false;
    this.listeners = [];
    this.lastFrame = 0;
    this.width = 1; this.height = 1;
    if (!canvas.hasAttribute('tabindex')) canvas.tabIndex = 0;
    if (!canvas.hasAttribute('aria-label')) canvas.setAttribute('aria-label', '共有文明の地図。クリックで選択、ダブルクリックで移動、ドラッグで地図を移動');
    canvas.style.touchAction = 'none';
    canvas.style.cursor = 'grab';
    this._bind();
    this.resizeObserver = new ResizeObserver(() => this._resize());
    this.resizeObserver.observe(canvas);
    this._resize();
    this.frame = requestAnimationFrame(t => this._frame(t));
  }

  _listen(target, type, handler, options) {
    target.addEventListener(type, handler, options);
    this.listeners.push(() => target.removeEventListener(type, handler, options));
  }

  _bind() {
    const point = event => {
      const rect = this.canvas.getBoundingClientRect();
      return { x: event.clientX - rect.left, y: event.clientY - rect.top };
    };
    this._listen(this.canvas, 'pointerdown', event => {
      if (event.button !== 0) return;
      const p = point(event);
      this.pointer = { id: event.pointerId, start: p, last: p, dragged: false };
      this.targetOffset = null;
      this.canvas.setPointerCapture(event.pointerId);
      this.canvas.style.cursor = 'grabbing';
    });
    this._listen(this.canvas, 'pointermove', event => {
      const p = point(event);
      if (this.pointer && this.pointer.id === event.pointerId) {
        if (Math.hypot(p.x - this.pointer.start.x, p.y - this.pointer.start.y) > 5) this.pointer.dragged = true;
        if (this.pointer.dragged) {
          this.offset.x += p.x - this.pointer.last.x;
          this.offset.y += p.y - this.pointer.last.y;
          this.dirty = true;
        }
        this.pointer.last = p;
      } else {
        const tile = this._hit(p.x, p.y);
        if (tile !== this.hover) { this.hover = tile; this.callbacks.onHover(tile); }
      }
    });
    this._listen(this.canvas, 'pointerup', event => {
      if (!this.pointer || this.pointer.id !== event.pointerId) return;
      const p = point(event);
      if (!this.pointer.dragged) {
        const tileId = this._hit(p.x, p.y);
        if (tileId) this.callbacks.onSelect(tileId);
      }
      this.pointer = null;
      if (this.canvas.hasPointerCapture(event.pointerId)) this.canvas.releasePointerCapture(event.pointerId);
      this.canvas.style.cursor = 'grab';
    });
    this._listen(this.canvas, 'pointercancel', () => { this.pointer = null; this.canvas.style.cursor = 'grab'; });
    this._listen(this.canvas, 'pointerleave', () => {
      if (!this.pointer) { this.hover = null; this.callbacks.onHover(null); }
    });
    this._listen(this.canvas, 'dblclick', event => {
      event.preventDefault();
      const p = point(event), tileId = this._hit(p.x, p.y);
      if (tileId) this.callbacks.onMove(tileId);
    });
    this._listen(this.canvas, 'wheel', event => {
      event.preventDefault();
      this._zoomAt(Math.exp(-clamp(event.deltaY, -140, 140) * .0015), point(event));
    }, { passive: false });
    this._listen(this.canvas, 'keydown', event => {
      const step = 65;
      const moves = { ArrowLeft: [step, 0], ArrowRight: [-step, 0], ArrowUp: [0, step], ArrowDown: [0, -step] };
      if (moves[event.key]) {
        event.preventDefault(); this.targetOffset = null;
        this.offset.x += moves[event.key][0]; this.offset.y += moves[event.key][1]; this.dirty = true;
      } else if (event.key === '+' || event.key === '=') this.zoomBy(1.15);
      else if (event.key === '-') this.zoomBy(1 / 1.15);
      else if (event.key.toLowerCase() === 'f') this.focusPlayer();
      else if (event.key === 'Enter' && this.selection) this.callbacks.onMove(this.selection);
    });
  }

  _usableCenter(forSelection = false) {
    if (this.width <= 850) return { x: this.width * .52, y: this.height * (forSelection ? .34 : .48) };
    return { x: this.width > 1000 ? this.width / 2 - 115 : this.width / 2,
      y: this.height / 2 + (this.height > 620 ? 12 : 0) };
  }

  _resize() {
    const rect = this.canvas.getBoundingClientRect();
    const width = Math.max(1, Math.round(rect.width)), height = Math.max(1, Math.round(rect.height));
    const oldCenter = this._usableCenter();
    this.width = width; this.height = height;
    const newCenter = this._usableCenter();
    this.dpr = Math.min(window.devicePixelRatio || 1, 2);
    this.canvas.width = this.cache.width = Math.round(width * this.dpr);
    this.canvas.height = this.cache.height = Math.round(height * this.dpr);
    if (this.initialized) {
      this.offset.x += newCenter.x - oldCenter.x;
      this.offset.y += newCenter.y - oldCenter.y;
    } else {
      this.offset = newCenter;
      this.zoom = width < 680 ? .85 : height < 680 ? 1 : 1.25;
    }
    this.targetOffset = null;
    this.dirty = true;
  }

  setState(world, actorId) {
    if (!world || !Array.isArray(world.tiles)) return;
    this.world = world; this.actorId = actorId;
    this.tiles = new Map(world.tiles.map(tile => [tile.id, tile]));
    this.sortedTiles = world.tiles.slice().sort((a, b) => a.r - b.r || a.q - b.q);
    const now = performance.now();
    const entities = [...Object.values(world.players || {}), ...(world.npcs || []), ...(world.caravans || []).map(entity => ({ ...entity, courier: true }))];
    const live = new Set();
    for (const entity of entities) {
      live.add(entity.id);
      const target = this._modelPosition(entity, 0);
      const previous = this.positions.has(entity.id) ? this._entityPosition(entity.id, now) : target;
      this.positions.set(entity.id, { entity, since: now, error: { x: previous.x - target.x, y: previous.y - target.y } });
    }
    for (const id of this.positions.keys()) if (!live.has(id)) this.positions.delete(id);
    if (!this.initialized) {
      const center = this._usableCenter();
      const own = world.players?.[actorId];
      const p = own ? project(own.q, own.r) : { x: 0, y: 0 };
      this.offset = { x: center.x - p.x * this.zoom, y: center.y - p.y * this.zoom };
      this.initialized = true;
    }
    this.dirty = true;
  }

  setSelection(tileId) { this.selection = this.tiles.has(tileId) ? tileId : null; }
  setLens(lens) {
    this.lens = ['normal', 'food', 'industry', 'logistics', 'danger'].includes(lens) ? lens : 'normal';
    this.dirty = true;
  }
  focusTile(tileId) {
    const tile = this.tiles.get(tileId);
    if (!tile) return;
    const p = project(tile.q, tile.r), c = this._usableCenter(true);
    this.targetOffset = { x: c.x - p.x * this.zoom, y: c.y - p.y * this.zoom };
  }
  focusPlayer() {
    const actor = this.world?.players?.[this.actorId];
    if (!actor) return;
    const p = project(actor.q, actor.r), c = this._usableCenter();
    this.targetOffset = { x: c.x - p.x * this.zoom, y: c.y - p.y * this.zoom };
  }
  zoomBy(factor) { if (Number.isFinite(factor) && factor > 0) this._zoomAt(factor, this._usableCenter()); }
  _zoomAt(factor, p) {
    const next = clamp(this.zoom * factor, .55, 2.6), ratio = next / this.zoom;
    this.offset.x = p.x - (p.x - this.offset.x) * ratio;
    this.offset.y = p.y - (p.y - this.offset.y) * ratio;
    this.zoom = next; this.targetOffset = null; this.dirty = true;
  }
  screenPosition(tileId) {
    const tile = this.tiles.get(tileId);
    if (!tile) return null;
    const p = project(tile.q, tile.r);
    return { x: p.x * this.zoom + this.offset.x, y: p.y * this.zoom + this.offset.y };
  }
  _hit(x, y) {
    const id = inverseHex((x - this.offset.x) / this.zoom, (y - this.offset.y) / this.zoom);
    return this.tiles.has(id) ? id : null;
  }

  _entityPosition(id, now) {
    const pos = this.positions.get(id);
    if (!pos) return { x: 0, y: 0 };
    const age = Math.max(0, now - pos.since);
    // Only animate a route the simulation has accepted. Prediction is bounded to
    // one polling interval, so disconnected clients never keep travelling forever.
    const target = this._modelPosition(pos.entity, Math.min(age, 700));
    const bridge = 1 - clamp(age / 320, 0, 1);
    return { x: target.x + pos.error.x * bridge, y: target.y + pos.error.y * bridge };
  }

  _modelPosition(entity, extraMs) {
    let q = Number(entity.q) || 0, r = Number(entity.r) || 0;
    if (entity.courier) {
      if (Array.isArray(entity.route) && entity.route.length > 1 && entity.durationMs) {
        const progress = clamp(((entity.elapsedMs || 0) + extraMs) / entity.durationMs, 0, 1);
        const segment = progress * (entity.route.length - 1), index = Math.floor(segment);
        const from = this.tiles.get(entity.route[index]);
        const to = this.tiles.get(entity.route[Math.min(index + 1, entity.route.length - 1)]);
        if (from && to) { q = from.q + (to.q - from.q) * (segment - index); r = from.r + (to.r - from.r) * (segment - index); }
      }
    } else if (entity.path?.length) {
      let elapsed = (Number(entity.moveProgress) || 0) + extraMs;
      for (const id of entity.path) {
        const next = this.tiles.get(id); if (!next) break;
        const duration = 900 * (next.road ? .65 : ({ grass: 1, forest: 1.25, hill: 1.6, mountain: 2.5 }[next.terrain] || 1));
        if (elapsed < duration) { q += (next.q - q) * elapsed / duration; r += (next.r - r) * elapsed / duration; break; }
        q = next.q; r = next.r; elapsed -= duration;
      }
    }
    return project(q, r);
  }

  _frame(now) {
    if (this.destroyed) return;
    this.frame = requestAnimationFrame(t => this._frame(t));
    // The map does not need a 120 Hz render loop; keep idle laptops cool.
    if (document.hidden || now - this.lastFrame < 32) return;
    this.lastFrame = now;
    if (this.targetOffset) {
      this.offset.x += (this.targetOffset.x - this.offset.x) * .2;
      this.offset.y += (this.targetOffset.y - this.offset.y) * .2;
      if (Math.hypot(this.offset.x - this.targetOffset.x, this.offset.y - this.targetOffset.y) < .6) {
        this.offset = this.targetOffset; this.targetOffset = null;
      }
      this.dirty = true;
    }
    if (this.dirty) { this._renderStatic(); this.dirty = false; }
    const ctx = this.ctx;
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.drawImage(this.cache, 0, 0);
    ctx.setTransform(this.dpr, 0, 0, this.dpr, 0, 0);
    if (!this.world) return;
    ctx.save(); ctx.translate(this.offset.x, this.offset.y); ctx.scale(this.zoom, this.zoom);
    this._drawHighlights(ctx, now);
    this._drawPaths(ctx, now);
    this._drawUnits(ctx, now);
    this._drawLabels(ctx);
    ctx.restore();
    this._drawCompass(ctx);
  }

  _visible(tile, padding = 120) {
    const p = this.screenPosition(tile.id);
    return p.x > -padding && p.y > -padding && p.x < this.width + padding && p.y < this.height + padding;
  }

  _renderStatic() {
    const ctx = this.cacheCtx;
    ctx.setTransform(this.dpr, 0, 0, this.dpr, 0, 0);
    const background = ctx.createRadialGradient(this.width * .45, this.height * .45, 40, this.width * .45, this.height * .45, Math.max(this.width, this.height) * .75);
    background.addColorStop(0, '#eee9d9'); background.addColorStop(.58, '#e6e4d5'); background.addColorStop(1, '#cfd8cd');
    ctx.fillStyle = background; ctx.fillRect(0, 0, this.width, this.height);
    // Subtle paper grain remains screen-fixed; it is not part of the simulation.
    ctx.fillStyle = 'rgba(90,111,96,.055)';
    for (let i = 0; i < 280; i++) ctx.fillRect(seed(i, 3) * this.width, seed(i, 7) * this.height, 1, 1);
    if (!this.world) return;
    ctx.save(); ctx.translate(this.offset.x, this.offset.y); ctx.scale(this.zoom, this.zoom);
    const tiles = this.sortedTiles.filter(tile => this._visible(tile));
    for (const tile of tiles) this._drawTile(ctx, tile);
    for (const tile of tiles) if (tile.explored && tile.road) this._drawRoad(ctx, tile);
    for (const tile of tiles) if (tile.explored) this._drawDecor(ctx, tile);
    for (const tile of tiles) this._drawLens(ctx, tile);
    ctx.restore();
  }

  _drawTile(ctx, tile) {
    const p = project(tile.q, tile.r), points = hexPoints(p.x, p.y, .65);
    const palette = COLORS[tile.explored ? tile.terrain : 'fog'] || COLORS.grass;
    const depth = tile.terrain === 'water' && tile.explored ? 3 : 7;
    const adjacentKnown = !tile.explored && NEIGHBORS.some(([q, r]) => this.tiles.get(key(tile.q + q, tile.r + r))?.explored);
    if (!tile.explored) ctx.globalAlpha = adjacentKnown ? .9 : .46;
    for (let i = 0; i < 3; i++) {
      const a = points[i], b = points[i + 1];
      polygon(ctx, [a, b, [b[0], b[1] + depth], [a[0], a[1] + depth]], i === 0 ? palette[1] : palette[2]);
    }
    const top = ctx.createLinearGradient(p.x - 30, p.y - 25, p.x + 30, p.y + 35);
    top.addColorStop(0, palette[0]); top.addColorStop(1, palette[1]);
    polygon(ctx, points, top, tile.explored ? 'rgba(239,237,208,.48)' : 'rgba(240,241,225,.3)', .8);
    if (!tile.explored) {
      if (adjacentKnown) {
        ctx.strokeStyle = 'rgba(244,242,226,.9)'; ctx.lineWidth = .6;
        ctx.beginPath(); ctx.moveTo(p.x - 7, p.y); ctx.lineTo(p.x + 7, p.y);
        ctx.moveTo(p.x, p.y - 5); ctx.lineTo(p.x, p.y + 5); ctx.stroke();
      }
      ctx.globalAlpha = 1; return;
    }
    ctx.save(); polygon(ctx, points); ctx.clip();
    if (tile.terrain === 'water') {
      ctx.strokeStyle = 'rgba(204,229,208,.5)'; ctx.lineWidth = 1.3;
      for (let i = 0; i < 5; i++) {
        const x = p.x - 35 + seed(tile.q, tile.r, i) * 55, y = p.y - 24 + i * 11;
        ctx.beginPath(); ctx.moveTo(x, y); ctx.bezierCurveTo(x + 6, y - 3, x + 12, y + 3, x + 23, y); ctx.stroke();
      }
    } else {
      for (let i = 0; i < 15; i++) {
        const x = p.x - 37 + seed(tile.q, tile.r, i + 1) * 74;
        const y = p.y - 28 + seed(tile.q, tile.r, i + 27) * 56;
        ctx.fillStyle = i % 3 === 0 ? 'rgba(246,234,181,.25)' : 'rgba(63,89,61,.10)';
        ctx.fillRect(x, y, 1.5 + seed(tile.q, tile.r, i + 50) * 3, 1.1);
      }
      if (tile.terrain === 'grass' && !tile.buildingId && !tile.road) {
        ctx.strokeStyle = 'rgba(83,117,65,.3)'; ctx.lineWidth = 1;
        for (let i = 0; i < 5; i++) {
          const x = p.x - 29 + seed(tile.q, tile.r, i + 91) * 58;
          const y = p.y - 19 + seed(tile.q, tile.r, i + 112) * 38;
          ctx.beginPath(); ctx.moveTo(x - 2, y - 3); ctx.lineTo(x, y); ctx.lineTo(x + 1, y - 4); ctx.stroke();
        }
      }
    }
    ctx.restore();
  }

  _drawRoad(ctx, tile) {
    const p = project(tile.q, tile.r);
    const connections = NEIGHBORS.map(([dq, dr]) => this.tiles.get(key(tile.q + dq, tile.r + dr)))
      .filter(other => other?.explored && (other.road || (other.buildingId && this.world.buildings?.[other.buildingId]?.connected)));
    for (const width of [10, 6.5, 1]) {
      ctx.strokeStyle = width === 10 ? 'rgba(116,105,77,.24)' : width === 6.5 ? '#d9cbb1' : 'rgba(246,232,195,.52)';
      ctx.lineWidth = width; ctx.lineCap = 'round';
      for (const other of connections) {
        const p2 = project(other.q, other.r);
        ctx.beginPath(); ctx.moveTo(p.x, p.y + 1); ctx.lineTo((p.x + p2.x) / 2, (p.y + p2.y) / 2 + 1); ctx.stroke();
      }
    }
    ctx.fillStyle = '#d9cbb1'; ctx.beginPath(); ctx.ellipse(p.x, p.y, 5, 3, 0, 0, Math.PI * 2); ctx.fill();
    ctx.lineCap = 'butt';
  }

  _drawDecor(ctx, tile) {
    const p = project(tile.q, tile.r);
    const building = this.world.buildings?.[tile.buildingId];
    if (tile.terrain === 'forest') {
      const positions = building ? [[-27, 2, .75], [24, -8, .72]] : [[-20, -5, .93], [5, -14, 1.03], [22, 2, .86], [-5, 14, .83]];
      positions.forEach(([x, y, scale], i) => this._tree(ctx, p.x + x, p.y + y, scale, seed(tile.q, tile.r, i)));
    }
    if (tile.terrain === 'mountain') {
      this._mountain(ctx, p.x - 17, p.y + 7, 42, .84);
      this._mountain(ctx, p.x + 9, p.y + 13, 67, 1.03);
      this._mountain(ctx, p.x + 26, p.y + 23, 28, .7);
    } else if (tile.terrain === 'hill') {
      this._rock(ctx, p.x - 20, p.y + 4, 19);
      this._rock(ctx, p.x + 15, p.y + 15, 13);
      if (tile.resource === 'ore') {
        for (let i = 0; i < 3; i++) polygon(ctx, [[p.x - 20 + i * 5, p.y - 6 + i * 2], [p.x - 17 + i * 5, p.y - 10 + i * 2], [p.x - 14 + i * 5, p.y - 6 + i * 2]], '#b47c4f');
      }
    }
    if (building) this._building(ctx, p.x, p.y, building);
  }

  _tree(ctx, x, y, s, variant) {
    ctx.save(); ctx.translate(x, y); ctx.scale(s, s);
    ctx.fillStyle = 'rgba(39,68,48,.18)'; ctx.beginPath(); ctx.ellipse(7, 3, 17, 6, -.1, 0, Math.PI * 2); ctx.fill();
    ctx.fillStyle = '#725c41'; ctx.fillRect(-2, -16, 4, 20);
    if (variant > .45) {
      polygon(ctx, [[-16, -10], [0, -46], [17, -10]], '#416b53');
      polygon(ctx, [[-13, -23], [0, -48], [1, -15]], '#6d8d61');
      polygon(ctx, [[-13, -7], [0, -35], [15, -7]], '#517954');
      polygon(ctx, [[-13, -7], [0, -35], [0, -5]], '#799760');
    } else {
      const green = variant < .16 ? ['#a0a365', '#7d8f57', '#617c53'] : ['#86a379', '#638b67', '#476f58'];
      ctx.fillStyle = green[2]; ctx.beginPath(); ctx.ellipse(4, -23, 18, 17, .2, 0, Math.PI * 2); ctx.fill();
      ctx.fillStyle = green[1]; ctx.beginPath(); ctx.ellipse(-5, -30, 17, 16, -.3, 0, Math.PI * 2); ctx.fill();
      ctx.fillStyle = green[0]; ctx.beginPath(); ctx.ellipse(-9, -35, 10, 9, -.3, 0, Math.PI * 2); ctx.fill();
      ctx.fillStyle = 'rgba(216,218,157,.28)'; ctx.beginPath(); ctx.ellipse(-12, -37, 5, 3, -.3, 0, Math.PI * 2); ctx.fill();
    }
    ctx.restore();
  }

  _mountain(ctx, x, y, height, scale) {
    ctx.save(); ctx.translate(x, y); ctx.scale(scale, scale);
    ctx.fillStyle = 'rgba(59,71,67,.16)'; ctx.beginPath(); ctx.ellipse(10, 3, 27, 9, .05, 0, Math.PI * 2); ctx.fill();
    polygon(ctx, [[-28, 2], [-4, -height], [10, -height * .6], [27, 2], [0, 12]], '#a8aa9b');
    polygon(ctx, [[-4, -height], [10, -height * .6], [27, 2], [0, 12], [1, -height * .55]], '#7d8a82');
    polygon(ctx, [[-28, 2], [-4, -height], [-7, -height * .36], [-16, -2]], '#c8c6b0');
    polygon(ctx, [[-11, -height * .69], [-4, -height], [5, -height * .74], [0, -height * .78], [-5, -height * .7]], '#e9e6d2');
    ctx.restore();
  }

  _rock(ctx, x, y, s) {
    polygon(ctx, [[x - s, y], [x - s * .45, y - s * .78], [x + s * .25, y - s], [x + s, y - s * .2], [x + s * .55, y + 4], [x - 3, y + 7]], '#9b9e8d');
    polygon(ctx, [[x - s, y], [x - s * .45, y - s * .78], [x + s * .25, y - s], [x - 3, y + 1]], '#c3c0a8');
    polygon(ctx, [[x + s * .25, y - s], [x + s, y - s * .2], [x + s * .55, y + 4], [x - 3, y + 7], [x - 3, y + 1]], '#878e80');
  }

  _house(ctx, x, y, w, h, roof = '#b4694c', tower = false) {
    const depth = w * .42;
    ctx.fillStyle = 'rgba(41,63,54,.15)'; ctx.beginPath(); ctx.ellipse(x + 8, y + 5, w * .8, 7, 0, 0, Math.PI * 2); ctx.fill();
    polygon(ctx, [[x - w / 2, y - h], [x + w / 2, y - h + 6], [x + w / 2, y + 5], [x - w / 2, y - 1]], '#e9d7ad', '#927b5c', .5);
    polygon(ctx, [[x + w / 2, y - h + 6], [x + w / 2 + depth, y - h - 3], [x + w / 2 + depth, y - 4], [x + w / 2, y + 5]], '#c5b996', '#927b5c', .5);
    polygon(ctx, [[x - w / 2 - 4, y - h], [x - 3, y - h - 18], [x + w / 2 + depth + 4, y - h - 11], [x + w / 2 + 3, y - h + 8]], roof, '#795d48', .6);
    polygon(ctx, [[x - w / 2 - 4, y - h], [x - 3, y - h - 18], [x + w / 2 + 3, y - h + 8]], '#cd8a5e', '#795d48', .6);
    ctx.strokeStyle = 'rgba(243,203,143,.3)'; ctx.lineWidth = .7;
    for (let i = 1; i < 4; i++) {
      const y1 = y - h - 17 + i * 5;
      ctx.beginPath(); ctx.moveTo(x - 3 - i * 4, y1); ctx.lineTo(x + w / 2 - 2, y1 + 10); ctx.stroke();
    }
    ctx.fillStyle = '#526a60'; ctx.fillRect(x - w / 2 + 5, y - h + 8, 5, 7);
    ctx.fillStyle = '#6a614a'; ctx.fillRect(x + 1, y - 9, 7, 12);
    ctx.fillStyle = '#d2b375'; ctx.fillRect(x + 2, y - 8, 1.5, 10);
    ctx.fillStyle = '#637469'; ctx.fillRect(x + w / 2 + 4, y - h + 6, 4, 6);
    if (tower) {
      ctx.fillStyle = '#c9bc97'; ctx.fillRect(x - 7, y - h - 39, 11, 27);
      polygon(ctx, [[x - 11, y - h - 37], [x - 2, y - h - 48], [x + 8, y - h - 35]], '#759084', '#476555', .6);
      ctx.fillStyle = '#5f7569'; ctx.fillRect(x - 3, y - h - 31, 3, 7);
    }
  }

  _building(ctx, x, y, b) {
    ctx.save();
    if (b.status === 'building') {
      ctx.globalAlpha = .38;
      this._buildingShape(ctx, x, y, b.type);
      ctx.globalAlpha = 1;
      ctx.strokeStyle = '#9f7951'; ctx.lineWidth = 2;
      for (const dx of [-26, 25]) { ctx.beginPath(); ctx.moveTo(x + dx, y + 8); ctx.lineTo(x + dx, y - 43); ctx.stroke(); }
      for (const dy of [-32, -16, 0]) { ctx.beginPath(); ctx.moveTo(x - 28, y + dy); ctx.lineTo(x + 28, y + dy); ctx.stroke(); }
      ctx.lineWidth = 1;
      ctx.beginPath(); ctx.moveTo(x - 26, y + 7); ctx.lineTo(x + 25, y - 32); ctx.stroke();
      rounded(ctx, x - 20, y + 12, 40, 5, 2); ctx.fillStyle = '#756d50'; ctx.fill();
      const progress = clamp(Number(b.progress) || 0, 0, 1);
      if (progress > 0) { rounded(ctx, x - 19, y + 13, Math.max(2, 38 * progress), 3, 1); ctx.fillStyle = '#e8c271'; ctx.fill(); }
    } else this._buildingShape(ctx, x, y, b.type);
    if (b.status === 'blocked' || b.connected === false) {
      ctx.fillStyle = '#ba7850'; ctx.beginPath(); ctx.arc(x + 27, y - 29, 6, 0, Math.PI * 2); ctx.fill();
      ctx.fillStyle = '#fff2d1'; ctx.font = 'bold 9px sans-serif'; ctx.textAlign = 'center'; ctx.fillText('!', x + 27, y - 26);
    }
    ctx.restore();
  }

  _buildingShape(ctx, x, y, type) {
    switch (type) {
      case 'townhall':
        polygon(ctx, [[x - 35, y - 3], [x, y - 19], [x + 38, y - 3], [x + 5, y + 20]], '#d5c9a9', '#b4ae93', 1);
        this._house(ctx, x - 5, y + 1, 36, 28, '#8b684f', true);
        ctx.fillStyle = '#ba915b'; ctx.fillRect(x - 33, y - 20, 1.5, 26);
        polygon(ctx, [[x - 31, y - 20], [x - 20, y - 16], [x - 31, y - 12]], '#d7bb78');
        break;
      case 'farm':
        for (let i = 0; i < 5; i++) {
          const px = x - 28 + i * 8, py = y + 3 + i * 1.5;
          polygon(ctx, [[px, py], [px + 18, py - 11], [px + 23, py - 9], [px + 5, py + 2]], '#9b8252');
          ctx.strokeStyle = '#d0bc68'; ctx.lineWidth = 3;
          ctx.beginPath(); ctx.moveTo(px + 2, py - 2); ctx.lineTo(px + 17, py - 11); ctx.stroke();
        }
        this._house(ctx, x - 12, y - 9, 19, 16, '#af7052');
        break;
      case 'lumbermill':
        this._house(ctx, x - 7, y - 2, 29, 23, '#8f6950');
        for (let i = 0; i < 3; i++) {
          rounded(ctx, x + 12, y + 4 - i * 5, 22, 5, 2); ctx.fillStyle = '#8e6f46'; ctx.fill();
          ctx.fillStyle = '#d9bb7c'; ctx.beginPath(); ctx.ellipse(x + 13, y + 6.5 - i * 5, 2, 2.5, 0, 0, Math.PI * 2); ctx.fill();
        }
        break;
      case 'quarry':
        polygon(ctx, [[x - 28, y - 3], [x, y - 17], [x + 28, y - 2], [x + 3, y + 16]], '#8d9588');
        this._rock(ctx, x - 14, y + 2, 14); this._rock(ctx, x + 15, y + 11, 10);
        ctx.strokeStyle = '#94754e'; ctx.lineWidth = 4;
        ctx.beginPath(); ctx.moveTo(x + 4, y + 4); ctx.lineTo(x + 4, y - 35); ctx.lineTo(x + 25, y - 29); ctx.stroke();
        ctx.strokeStyle = '#5a665a'; ctx.lineWidth = 1;
        ctx.beginPath(); ctx.moveTo(x + 22, y - 30); ctx.lineTo(x + 22, y - 13); ctx.stroke();
        break;
      case 'mine':
        this._rock(ctx, x, y + 6, 29);
        polygon(ctx, [[x - 14, y + 4], [x - 13, y - 17], [x + 7, y - 19], [x + 13, y - 11], [x + 13, y + 8]], '#586b61', '#a58357', 4);
        ctx.strokeStyle = '#777b6d'; ctx.lineWidth = 2;
        for (let i = 0; i < 2; i++) { ctx.beginPath(); ctx.moveTo(x - 8 + i * 12, y + 7); ctx.lineTo(x - 18 + i * 12, y + 19); ctx.stroke(); }
        break;
      case 'workshop':
        this._house(ctx, x - 10, y, 29, 23, '#9b5e47');
        polygon(ctx, [[x + 19, y + 5], [x + 19, y - 28], [x + 27, y - 31], [x + 31, y - 28], [x + 31, y]], '#a2856b', '#7f7661', 1);
        ctx.fillStyle = '#705e4b'; ctx.fillRect(x + 22, y - 30, 5, 3);
        ctx.fillStyle = '#e5ae62'; ctx.fillRect(x + 21, y - 3, 5, 6);
        break;
      case 'watchtower':
        ctx.strokeStyle = '#89704d'; ctx.lineWidth = 4;
        for (const dx of [-12, 12]) { ctx.beginPath(); ctx.moveTo(x + dx, y + 5); ctx.lineTo(x + dx * .7, y - 34); ctx.stroke(); }
        ctx.lineWidth = 2; ctx.beginPath(); ctx.moveTo(x - 11, y + 3); ctx.lineTo(x + 8, y - 30); ctx.moveTo(x + 11, y + 3); ctx.lineTo(x - 8, y - 30); ctx.stroke();
        this._house(ctx, x - 2, y - 27, 24, 12, '#688579');
        break;
      case 'archive':
        this._house(ctx, x - 7, y + 3, 34, 28, '#688985');
        ctx.fillStyle = '#d5c7a6'; ctx.fillRect(x + 2, y - 43, 12, 14);
        ctx.fillStyle = '#6c938b'; ctx.beginPath(); ctx.ellipse(x + 8, y - 43, 9, 10, 0, Math.PI, 0); ctx.fill();
        ctx.strokeStyle = '#bba263'; ctx.lineWidth = 1; ctx.beginPath(); ctx.moveTo(x + 8, y - 53); ctx.lineTo(x + 8, y - 59); ctx.stroke();
        break;
      default: this._house(ctx, x - 5, y, 27, 24);
    }
  }

  _drawLens(ctx, tile) {
    if (this.lens === 'normal' || !tile.explored) return;
    const b = this.world.buildings?.[tile.buildingId], p = project(tile.q, tile.r);
    let fill = null, stroke = null, text = null;
    if (this.lens === 'food' && (tile.terrain === 'grass' || b?.type === 'farm')) {
      fill = b?.type === 'farm' ? 'rgba(210,184,69,.26)' : 'rgba(208,221,113,.13)';
      stroke = '#d9cb78'; text = b?.type === 'farm' ? '食' : null;
    } else if (this.lens === 'industry' && (tile.resource || ['lumbermill', 'quarry', 'mine', 'workshop'].includes(b?.type))) {
      fill = 'rgba(196,139,87,.18)'; stroke = '#d4ad7a';
      text = { wood: '木', stone: '石', ore: '鉄' }[tile.resource] || '工';
    } else if (this.lens === 'logistics' && (tile.road || b)) {
      fill = b?.connected === false ? 'rgba(174,95,67,.26)' : 'rgba(80,152,156,.17)';
      stroke = b?.connected === false ? '#d39b75' : '#c0dfd2';
      text = b?.connected === false ? '未接続' : null;
    } else if (this.lens === 'danger' && ['water', 'mountain'].includes(tile.terrain)) {
      fill = tile.terrain === 'water' ? 'rgba(123,103,86,.26)' : 'rgba(185,154,82,.12)';
      stroke = tile.terrain === 'water' ? '#b29780' : '#c6b47d';
    }
    if (fill) polygon(ctx, hexPoints(p.x, p.y, 3), fill, stroke, 1.6);
    if (text && !b) {
      rounded(ctx, p.x - 11, p.y + 9, 22, 18, 5); ctx.fillStyle = 'rgba(48,75,61,.86)'; ctx.fill();
      ctx.font = 'bold 10px sans-serif'; ctx.fillStyle = '#f1e8cd'; ctx.textAlign = 'center'; ctx.fillText(text, p.x, p.y + 22);
    }
  }

  _drawHighlights(ctx, now) {
    for (const [id, selected] of [[this.hover, false], [this.selection, true]]) {
      if (!id || (!selected && id === this.selection)) continue;
      const tile = this.tiles.get(id); if (!tile) continue;
      const p = project(tile.q, tile.r);
      polygon(ctx, hexPoints(p.x, p.y, 2.5), selected ? 'rgba(249,220,139,.10)' : 'rgba(255,252,225,.10)', selected ? '#ffedb0' : 'rgba(255,248,217,.7)', selected ? 2.4 / this.zoom : 1.4 / this.zoom);
      if (selected) {
        ctx.save(); ctx.shadowBlur = 8; ctx.shadowColor = 'rgba(221,190,112,.35)';
        const corners = hexPoints(p.x, p.y, 0);
        for (const [x, y] of corners) { ctx.fillStyle = '#ffedb0'; ctx.beginPath(); ctx.arc(x, y, 2.3 / this.zoom, 0, Math.PI * 2); ctx.fill(); }
        ctx.restore();
      }
    }
    if (this.lens === 'logistics') {
      ctx.strokeStyle = 'rgba(231,241,191,.48)'; ctx.lineWidth = 1.5; ctx.setLineDash([3, 7]); ctx.lineDashOffset = -now / 170;
      for (const caravan of this.world.caravans || []) {
        const a = this.tiles.get(caravan.fromTileId), b = this.tiles.get(caravan.toTileId);
        if (!a?.explored || !b?.explored) continue;
        const route = (caravan.route || [a.id, b.id]).map(id => this.tiles.get(id)).filter(Boolean);
        ctx.beginPath(); route.forEach((tile, index) => { const point = project(tile.q, tile.r); if (index) ctx.lineTo(point.x, point.y); else ctx.moveTo(point.x, point.y); }); ctx.stroke();
      }
      ctx.setLineDash([]);
    }
  }

  _drawPaths(ctx, now) {
    const actor = this.world.players?.[this.actorId];
    if (!actor?.path?.length) return;
    const p = this._entityPosition(actor.id, now);
    ctx.beginPath(); ctx.moveTo(p.x, p.y);
    for (const id of actor.path) {
      const tile = this.tiles.get(typeof id === 'string' ? id : id?.tileId || key(id?.q, id?.r));
      if (tile) { const next = project(tile.q, tile.r); ctx.lineTo(next.x, next.y); }
    }
    ctx.strokeStyle = '#f4e3a4'; ctx.lineWidth = 2; ctx.setLineDash([2, 6]); ctx.lineDashOffset = -now / 120; ctx.stroke(); ctx.setLineDash([]);
  }

  _drawUnits(ctx, now) {
    const entities = [...Object.values(this.world.players || {}).map(e => ({ ...e, player: true })), ...(this.world.npcs || [])];
    entities.sort((a, b) => a.r - b.r);
    for (const entity of entities) {
      const tile = this.tiles.get(inverseHex(project(entity.q, entity.r).x, project(entity.q, entity.r).y));
      if (tile && !tile.explored) continue;
      const p = this._entityPosition(entity.id, now);
      const own = entity.id === this.actorId;
      const moving = !!entity.path?.length || /moving|travel|walk|haul|deliver/i.test(entity.status || '');
      const bob = moving ? Math.sin(now / 115 + seed(entity.q, entity.r) * 4) * 1.1 : Math.sin(now / 1200) * .3;
      const spread = own ? 0 : ((entity.id?.length || 1) % 3 - 1) * 7;
      const x = p.x + spread, y = p.y + (own ? 5 : -5);
      if (own) {
        ctx.strokeStyle = '#ffe6a1'; ctx.lineWidth = 2;
        ctx.fillStyle = 'rgba(235,201,114,.18)'; ctx.beginPath(); ctx.ellipse(x, y + 2, 12 + Math.sin(now / 600), 6, 0, 0, Math.PI * 2); ctx.fill(); ctx.stroke();
      }
      ctx.fillStyle = 'rgba(43,65,52,.24)'; ctx.beginPath(); ctx.ellipse(x + 2, y + 2, 7, 3, 0, 0, Math.PI * 2); ctx.fill();
      ctx.save(); ctx.translate(x, y + bob);
      const stride = moving ? Math.sin(now / 100) * 2.3 : 0;
      ctx.strokeStyle = '#454d42'; ctx.lineWidth = 2.5; ctx.lineCap = 'round';
      ctx.beginPath(); ctx.moveTo(-2, -6); ctx.lineTo(-3 - stride, 1); ctx.moveTo(2, -6); ctx.lineTo(3 + stride, 1); ctx.stroke();
      polygon(ctx, [[-5, -17], [3, -18], [6, -6], [-5, -5]], own ? '#e9bd65' : entity.player ? '#9c87aa' : '#e1d6b2', '#526653', .6);
      ctx.strokeStyle = own ? '#b18e53' : '#839181'; ctx.lineWidth = 2;
      ctx.beginPath(); ctx.moveTo(-4, -15); ctx.lineTo(-7, -8 + stride); ctx.moveTo(4, -15); ctx.lineTo(7, -8 - stride); ctx.stroke();
      ctx.fillStyle = '#d9af80'; ctx.beginPath(); ctx.arc(0, -22, 4.3, 0, Math.PI * 2); ctx.fill();
      ctx.fillStyle = own ? '#65533c' : '#6c7257'; ctx.beginPath(); ctx.ellipse(-.6, -24, 4.7, 2.4, -.12, Math.PI, Math.PI * 2); ctx.fill();
      if (!entity.player) { ctx.fillStyle = '#947751'; ctx.fillRect(-6, -20, 12, 1.5); }
      if (entity.cargo) {
        polygon(ctx, [[-9, -14], [-3, -16], [0, -13], [0, -5], [-8, -4]], '#b98e56', '#745f3c', .7);
        ctx.strokeStyle = '#e0bd7c'; ctx.lineWidth = 1; ctx.beginPath(); ctx.moveTo(-8, -10); ctx.lineTo(-1, -11); ctx.stroke();
      }
      ctx.lineCap = 'butt'; ctx.restore();
      if (own) {
        const name = `${entity.name || 'あなた'}`;
        ctx.font = '600 10px system-ui, sans-serif'; const width = ctx.measureText(name).width + 16;
        rounded(ctx, x - width / 2, y - 48, width, 17, 6); ctx.fillStyle = '#284d40'; ctx.fill();
        ctx.fillStyle = '#fff1cc'; ctx.textAlign = 'center'; ctx.fillText(name, x, y - 36);
        polygon(ctx, [[x - 3, y - 31], [x + 3, y - 31], [x, y - 27]], '#284d40');
      }
    }
    for (const caravan of this.world.caravans || []) {
      const a = this.tiles.get(caravan.fromTileId), b = this.tiles.get(caravan.toTileId);
      if (!a?.explored || !b?.explored) continue;
      const position = this._entityPosition(caravan.id, now);
      const x = position.x, y = position.y;
      ctx.fillStyle = 'rgba(39,69,51,.2)'; ctx.beginPath(); ctx.ellipse(x + 2, y + 4, 11, 4, 0, 0, Math.PI * 2); ctx.fill();
      for (const dx of [-6, 6]) { ctx.fillStyle = '#695d45'; ctx.beginPath(); ctx.arc(x + dx, y + 2, 3, 0, Math.PI * 2); ctx.fill(); }
      polygon(ctx, [[x - 9, y - 8], [x + 5, y - 11], [x + 10, y - 7], [x + 10, y], [x - 8, y + 1]], '#c6a568', '#8a734e', .7);
      polygon(ctx, [[x - 6, y - 9], [x + 4, y - 12], [x + 7, y - 8], [x - 3, y - 6]], '#e3c88e');
      ctx.strokeStyle = '#7b704e'; ctx.lineWidth = 1; ctx.beginPath(); ctx.moveTo(x - 2, y - 10); ctx.lineTo(x + 2, y); ctx.stroke();
    }
  }

  _drawLabels(ctx) {
    if (this.zoom < .72) return;
    for (const building of Object.values(this.world.buildings || {})) {
      const tile = this.tiles.get(building.tileId);
      if (!tile?.explored || !this._visible(tile)) continue;
      const p = project(tile.q, tile.r);
      const text = LABELS[building.type] || building.type;
      ctx.font = `${building.type === 'townhall' ? '600' : '500'} 9px system-ui, sans-serif`;
      const width = ctx.measureText(text).width + 16;
      const y = p.y + (building.status === 'building' ? 21 : 17);
      rounded(ctx, p.x - width / 2, y, width, 17, 5); ctx.fillStyle = building.type === 'townhall' ? 'rgba(43,74,60,.95)' : 'rgba(248,242,220,.93)'; ctx.fill();
      ctx.strokeStyle = 'rgba(116,119,83,.2)'; ctx.lineWidth = .6; ctx.stroke();
      ctx.fillStyle = building.type === 'townhall' ? '#f4e5b8' : '#425747'; ctx.textAlign = 'center'; ctx.fillText(text, p.x, y + 11.5);
    }
  }

  _drawCompass(ctx) {
    if (this.width < 800 || this.height < 620) return;
    const x = 47, y = this.height - 160;
    ctx.save(); ctx.translate(x, y); ctx.globalAlpha = .48;
    ctx.strokeStyle = '#576f60'; ctx.lineWidth = .8; ctx.beginPath(); ctx.arc(0, 0, 17, 0, Math.PI * 2); ctx.stroke();
    polygon(ctx, [[0, -22], [-4, 3], [0, -2], [4, 3]], '#476552');
    polygon(ctx, [[0, 20], [-3, -1], [0, 3], [3, -1]], '#87977a');
    ctx.font = '500 8px Georgia, serif'; ctx.fillStyle = '#476552'; ctx.textAlign = 'center'; ctx.fillText('N', 0, -29);
    ctx.restore();
  }

  destroy() {
    this.destroyed = true;
    cancelAnimationFrame(this.frame);
    this.resizeObserver.disconnect();
    for (const remove of this.listeners) remove();
    this.listeners = [];
  }
}
