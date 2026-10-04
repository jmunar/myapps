// Column editor shared by the form-type create and edit pages.
//
// Both pages render `#columns-editor`; everything that used to differ between
// them (the form id, the hidden input the JSON lands in, the translated
// labels) arrives on that element as data attributes, so this file is the same
// for both and needs no values interpolated into it.
//
// Rows are built with DOM calls rather than innerHTML: the labels come back
// out of the dataset already entity-decoded, and re-parsing them as markup
// would decode them a second time.

(function () {
  var editor = document.getElementById('columns-editor');
  if (!editor) return;

  var cfg = editor.dataset;

  function option(value, label) {
    var opt = document.createElement('option');
    opt.value = value;
    opt.textContent = label;
    return opt;
  }

  function buildRow() {
    var row = document.createElement('div');
    row.className = 'ci-column-row';

    var name = document.createElement('input');
    name.type = 'text';
    name.placeholder = cfg.phName;
    name.required = true;
    name.setAttribute('data-col-name', '');

    var type = document.createElement('select');
    type.setAttribute('data-col-type', '');
    type.appendChild(option('text', cfg.tText));
    type.appendChild(option('number', cfg.tNumber));
    type.appendChild(option('bool', cfg.tBool));
    type.appendChild(option('link', cfg.tLink));

    var toggle = document.createElement('label');
    toggle.className = 'ci-col-multiline-toggle';
    var checkbox = document.createElement('input');
    checkbox.type = 'checkbox';
    checkbox.setAttribute('data-col-multiline', '');
    var span = document.createElement('span');
    span.textContent = cfg.tMultiline;
    toggle.appendChild(checkbox);
    toggle.appendChild(span);

    var remove = document.createElement('button');
    remove.type = 'button';
    remove.className = 'btn-icon btn-icon-danger';
    remove.setAttribute('data-col-remove', '');
    remove.textContent = '×';

    row.appendChild(name);
    row.appendChild(type);
    row.appendChild(toggle);
    row.appendChild(remove);
    return row;
  }

  // The multiline flag only means anything for text columns, so it is hidden
  // (and cleared) for every other type.
  function wireMultilineToggle(row) {
    var typeEl = row.querySelector('[data-col-type]');
    var toggleLabel = row.querySelector('.ci-col-multiline-toggle');
    var checkbox = row.querySelector('[data-col-multiline]');
    if (!typeEl || !toggleLabel || !checkbox) return;
    function refresh() {
      var isText = typeEl.value === 'text';
      toggleLabel.style.visibility = isText ? '' : 'hidden';
      if (!isText) checkbox.checked = false;
    }
    typeEl.addEventListener('change', refresh);
    refresh();
  }

  function serializeColumns() {
    var out = [];
    editor.querySelectorAll('.ci-column-row').forEach(function (row) {
      var nameEl = row.querySelector('[data-col-name]');
      var typeEl = row.querySelector('[data-col-type]');
      var multilineEl = row.querySelector('[data-col-multiline]');
      var name = nameEl ? nameEl.value.trim() : '';
      if (!name) return;
      var type = typeEl ? typeEl.value : 'text';
      var entry = { name: name, type: type };
      if (type === 'text' && multilineEl && multilineEl.checked) {
        entry.multiline = true;
      }
      out.push(entry);
    });
    return JSON.stringify(out);
  }

  editor.querySelectorAll('.ci-column-row').forEach(wireMultilineToggle);

  // Delegated, so it covers the rows rendered server-side and the ones added
  // here without either having to carry an inline handler.
  editor.addEventListener('click', function (e) {
    var btn = e.target.closest('[data-col-remove]');
    if (btn && editor.contains(btn)) {
      var row = btn.closest('.ci-column-row');
      if (row) row.remove();
    }
  });

  var addBtn = document.querySelector('[data-add-column]');
  if (addBtn) {
    addBtn.addEventListener('click', function () {
      var row = buildRow();
      editor.appendChild(row);
      wireMultilineToggle(row);
    });
  }

  var form = document.getElementById(cfg.form);
  var target = document.getElementById(cfg.target);
  if (form && target) {
    form.addEventListener('submit', function () {
      target.value = serializeColumns();
    });
  }
})();
