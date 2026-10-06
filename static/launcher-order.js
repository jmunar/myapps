// Drag-to-reorder for the launcher's edit mode.
//
// The edit grid arrives by htmx swap after this runs, so everything is
// delegated from the document. A card ([data-launcher-key]) is dragged by its
// handle ([data-launcher-handle]); it moves in the DOM as the pointer passes
// over the other cards, and on release the new order is posted to
// /launcher/order. Arrow keys on a focused handle move its card one place.
// The handle is a <button>, which the tab swipe already leaves alone, and has
// `touch-action: none` in core.css so a vertical drag is not taken for a
// scroll.

(function() {
    var BASE = document.documentElement.getAttribute('data-base') || '';
    var EDGE = 56;  // px from the top or bottom of the viewport where a drag scrolls
    var STEP = 12;  // px scrolled per pointermove there

    var drag = null;  // { card, pointerId, moved }
    // Saves go out one after another, so a slower earlier one cannot land last.
    var saving = Promise.resolve();

    function cards(grid) {
        return Array.prototype.slice.call(grid.querySelectorAll('[data-launcher-key]'));
    }

    function save(grid) {
        var order = cards(grid).map(function(c) {
            return c.getAttribute('data-launcher-key');
        });
        var body = 'order=' + encodeURIComponent(order.join(','));
        saving = saving.then(function() {
            return fetch(BASE + '/launcher/order', {
                method: 'POST',
                headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
                body: body,
                credentials: 'same-origin'
            });
        }).catch(function() {});
    }

    function cardAt(grid, x, y) {
        var list = cards(grid);
        for (var i = 0; i < list.length; i++) {
            var r = list[i].getBoundingClientRect();
            if (x >= r.left && x <= r.right && y >= r.top && y <= r.bottom) return list[i];
        }
        return null;
    }

    function handleOf(target) {
        return target && target.closest ? target.closest('[data-launcher-handle]') : null;
    }

    document.addEventListener('pointerdown', function(e) {
        var handle = handleOf(e.target);
        if (!handle || e.button !== 0) return;
        var card = handle.closest('[data-launcher-key]');
        if (!card) return;
        e.preventDefault();
        handle.setPointerCapture(e.pointerId);
        drag = { card: card, pointerId: e.pointerId, moved: false };
        card.classList.add('launcher-dragging');
    });

    document.addEventListener('pointermove', function(e) {
        if (!drag || e.pointerId !== drag.pointerId) return;
        e.preventDefault();
        if (e.clientY < EDGE) window.scrollBy(0, -STEP);
        else if (e.clientY > window.innerHeight - EDGE) window.scrollBy(0, STEP);

        var grid = drag.card.parentNode;
        var over = cardAt(grid, e.clientX, e.clientY);
        if (!over || over === drag.card) return;
        var list = cards(grid);
        // Take the place of the card passed over, from whichever side.
        if (list.indexOf(over) < list.indexOf(drag.card)) {
            grid.insertBefore(drag.card, over);
        } else {
            grid.insertBefore(drag.card, over.nextSibling);
        }
        drag.moved = true;
    });

    function end(e) {
        if (!drag || e.pointerId !== drag.pointerId) return;
        drag.card.classList.remove('launcher-dragging');
        if (drag.moved) save(drag.card.parentNode);
        drag = null;
    }
    document.addEventListener('pointerup', end);
    document.addEventListener('pointercancel', end);

    document.addEventListener('keydown', function(e) {
        var handle = handleOf(e.target);
        if (!handle) return;
        var card = handle.closest('[data-launcher-key]');
        if (!card) return;
        var grid = card.parentNode;
        var list = cards(grid);
        var i = list.indexOf(card);
        if ((e.key === 'ArrowUp' || e.key === 'ArrowLeft') && i > 0) {
            grid.insertBefore(card, list[i - 1]);
        } else if ((e.key === 'ArrowDown' || e.key === 'ArrowRight') && i < list.length - 1) {
            grid.insertBefore(card, list[i + 1].nextSibling);
        } else {
            return;
        }
        e.preventDefault();
        handle.focus();
        save(grid);
    });
})();
