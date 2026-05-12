# FormInput

Record structured data with custom forms. Define custom form types,
manage row sets, and capture rows of data with a spreadsheet-style grid.

## Screenshots

<p align="center">
  <img src="../../docs/screenshots/form-input-inputs.png" width="270" alt="Inputs" />
  <img src="../../docs/screenshots/form-input-view.png" width="270" alt="Input view" />
  <img src="../../docs/screenshots/form-input-row-sets.png" width="270" alt="Row sets" />
  <img src="../../docs/screenshots/form-input-form-types.png" width="270" alt="Form types" />
</p>

## Features

- Define custom form types with configurable columns (text, number, yes/no, link)
- Mark text columns as multi-line — values render on their own row beneath the
  main row both in the entry grid and the view, with a textarea editor
- Toggle `fixed_rows` per form type — pin rows to a row set, or add rows freely
- Manage row sets (named lists of row identifiers)
- Capture inputs as CSV-backed grids; double-click any cell to edit in place
- Bulk-create inputs by uploading a CSV (column count is enforced against the
  form type; for fixed-row form types the first column is the row-set key)
- Sort by any column (A→Z / Z→A) and filter rows with a single global search
  box that matches every cell's text — presentation-only, the underlying CSV
  stays unsorted/unfiltered
- Link cells: small modal for URL + display text, rendered as an anchor in
  the view
