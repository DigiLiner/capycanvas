# Brush design records

[Design history](../README.md)

This directory holds accepted designs and decision history for brush families
and shared stroke processing. Each record gives the intended equations, authoring
controls, approximations and alternatives. Clearly marked implementation
candidates are provisional snapshots; accepted designs do not establish which
features have shipped. Current behavior belongs in the
[brush guide](../../internals/brushes.md) and
[contact engine guide](../../internals/contact-brush-engine.md).

| Area | Design record |
| --- | --- |
| Dry media: pencil, charcoal, chalk, pastel and crayon | [Analytic relief and paper contact](dry-media-contact.md) |
| Blenders and the paper | [Paper-aware blenders](paper-aware-blenders.md) |
| Shared stroke stabilization | [Full-horizon reference objective](stroke-stabilization.md) |
| Curve-fit stabilization | [Implemented curve model and open tuning](curve-fit-work-in-progress.md) |

The earlier [pencil, charcoal and ink redesign](../dry-media-brush-design.md)
provides broader context for stroke generation and the existing contact engine.
Add other brush and stroke records here as their designs are decided. Measurements,
captures and implementation work logs stay outside these records.
