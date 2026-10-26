# 0002. Dual license: MIT OR Apache-2.0

- Status: Accepted
- Date: 2026-10-26

## Context

buffer needs a license before code lands, and the license constrains which dependencies it may
take.

## Decision

- buffer is licensed `MIT OR Apache-2.0`, set once as `license` in `[workspace.package]`.
- The repo root carries `LICENSE-APACHE` (the verbatim apache.org text) and `LICENSE-MIT` (the
  SPDX text, `Copyright (c) 2026 miles`).

## Consequences

- Every dependency must carry a license compatible with this choice. The allow list and the
  `cargo deny` enforcement live in the dependency policy ([0026](0026-dependency-policy.md)); GPL,
  LGPL, AGPL, SSPL, and non-commercial licenses are not allowed.
- Bundled fonts bring their own licenses (OFL-1.1, Ubuntu-font-1.0); see
  [0005](0005-eframe-features.md).
