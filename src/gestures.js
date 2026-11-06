// Swipe-left and long-press for rows marked with [data-kind].
// Vertical drags are left to the browser (touch-action: pan-y in CSS) so scrolling still works.

const HOLD_MS = 450;
const SLOP = 10; // px of movement before we decide what the gesture is

export function initGestures({ swipeLabel, onSwipe, onHold }) {
  let g = null; // current gesture
  let suppressClick = false;

  const reset = () => {
    if (!g) return;
    clearTimeout(g.timer);
    g.under?.remove();
    g = null;
  };

  document.addEventListener('pointerdown', (e) => {
    if (e.button !== 0) return;
    suppressClick = false;
    const el = e.target.closest('[data-kind]');
    if (!el || e.target.closest('input, select, textarea, dialog')) return;
    // ignore swipes starting at the very left edge (system back gesture)
    const edge = e.clientX < 16;
    g = { el, id: e.pointerId, x: e.clientX, y: e.clientY, mode: null, dx: 0, edge };
    g.timer = setTimeout(() => {
      if (!g || g.mode) return;
      g.mode = 'hold';
      suppressClick = true;
      el.classList.add('held');
      setTimeout(() => el.classList.remove('held'), 200);
      onHold(el);
    }, HOLD_MS);
  });

  document.addEventListener('pointermove', (e) => {
    if (!g || e.pointerId !== g.id || g.mode === 'hold') return;
    const dx = e.clientX - g.x;
    const dy = e.clientY - g.y;
    if (!g.mode) {
      if (Math.abs(dy) > SLOP && Math.abs(dy) > Math.abs(dx)) { reset(); return; } // it's a scroll
      if (Math.abs(dx) < SLOP) return;
      clearTimeout(g.timer);
      const label = swipeLabel(g.el);
      if (dx > 0 || g.edge || !label) { reset(); return; }
      g.mode = 'swipe';
      g.el.setPointerCapture?.(e.pointerId);
      const r = g.el.getBoundingClientRect();
      g.width = r.width;
      g.under = document.createElement('div');
      g.under.className = 'swipe-under';
      g.under.textContent = label;
      Object.assign(g.under.style, { top: `${r.top}px`, left: `${r.left}px`, width: `${r.width}px`, height: `${r.height}px` });
      document.body.appendChild(g.under);
      g.el.classList.add('swiping');
    }
    if (g.mode === 'swipe') {
      g.dx = Math.min(0, dx);
      g.el.style.transform = `translateX(${g.dx}px)`;
      g.under.classList.toggle('armed', -g.dx > threshold(g));
    }
  });

  const threshold = (gest) => Math.min(120, gest.width * 0.35);

  const end = (e) => {
    if (!g || e.pointerId !== g.id) return;
    // the click (if any) that follows this pointerup is swallowed; after that, clicks work again
    if (g.mode) setTimeout(() => { suppressClick = false; }, 0);
    if (g.mode === 'swipe') {
      suppressClick = true;
      const { el, under } = g;
      const commit = -g.dx > threshold(g);
      el.style.transition = 'transform .18s ease-out';
      el.style.transform = commit ? `translateX(-${g.width + 20}px)` : '';
      g.under = null; // keep it during the animation
      setTimeout(() => {
        under.remove();
        el.style.transition = '';
        el.classList.remove('swiping');
        if (commit) onSwipe(el); else el.style.transform = '';
      }, 180);
    }
    reset();
  };
  document.addEventListener('pointerup', end);
  document.addEventListener('pointercancel', (e) => {
    if (g?.el) { g.el.style.transform = ''; g.el.classList.remove('swiping'); }
    end(e);
  });

  // a swipe or hold must not also count as a tap on whatever is underneath
  document.addEventListener('click', (e) => {
    if (!suppressClick) return;
    suppressClick = false;
    e.preventDefault();
    e.stopPropagation();
  }, true);

  // links are natively draggable on desktop, which would cancel the swipe
  document.addEventListener('dragstart', (e) => {
    if (e.target.closest?.('[data-kind]')) e.preventDefault();
  });

  // right-click on a computer, and Android's own long-press, open the same actions
  document.addEventListener('contextmenu', (e) => {
    const el = e.target.closest('[data-kind]');
    if (!el) return;
    e.preventDefault();
    if (g?.mode === 'hold') return; // already opened by the timer
    reset();
    onHold(el);
  });
}
