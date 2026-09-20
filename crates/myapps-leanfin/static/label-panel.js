// Expanding detail panel on the Labels page.
//
// Only one frame is ever open: opening any panel clears every other slot
// first, and clicking the open trigger again closes it. Delegated from the
// document so it covers triggers that arrive later over HTMX.

(function () {
  document.addEventListener('click', function (e) {
    var btn = e.target.closest('[data-lf-panel]');
    if (!btn) return;

    var group = btn.closest('.lf-group');
    if (!group) return;
    var slot = group.querySelector('.lf-detail');
    var wasOpen = btn.classList.contains('lf-open');

    document.querySelectorAll('.lf-detail').forEach(function (d) {
      d.innerHTML = '';
    });
    document.querySelectorAll('.lf-open').forEach(function (b) {
      b.classList.remove('lf-open');
    });

    if (wasOpen) return;
    btn.classList.add('lf-open');
    htmx.ajax('GET', btn.dataset.url, slot);
  });
})();
