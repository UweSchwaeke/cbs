# Design Addendums

Design documents in this directory are **immutable point-in-time references**:
they record what was decided when they were written and are not edited after the
fact. When implementation later deviates from a design doc, the deviation is
recorded here instead of by changing the original — keeping the historical
record intact while still giving readers a single place to learn that a design
has been superseded or refined.

Each entry names the design doc it amends (by sequence number and title), the
date, and what changed and why. Read the original design doc first, then check
here for any addendum that applies to it.

---

## 002 — cbsd Rust port design ("Component Distribution")

**Date:** 2026-06-24

**Original text:** Doc 002's "Component Distribution" section says, on each
build dispatch, "the server packs the **relevant component directory** into a
gzip tarball and sends it to the worker" — phrased in the singular, as if each
build references one component.

**Deviation:** A `BuildDescriptor` may reference **multiple** components
(`components: Vec<BuildComponent>`; the Python reference and cbscore have always
supported this). Dispatch now packs **every** referenced component directory
into the one tarball, each under its own `<name>/` top-level prefix, rather than
only the first.

**Why this needs no protocol or worker change:** the worker unpacks the single
tarball into one directory and passes that directory to cbscore as
`component_path`; cbscore's `load_components` discovers components by
enumerating the **subdirectories** of that path (one `cbs.component.yaml` per
subdir). Placing one subdir per component in the tarball is therefore exactly
the layout cbscore already expects. The `build_new` message still carries a
single `component_sha256`, now computed over the combined archive.

**Bug fixed:** before this change, only `descriptor.components.first()` was
packed, so any second or later component never reached the worker and cbscore
aborted with `unknown component '<name>' specified`.

**Related:** in the same area, a dispatch tarball-pack failure is now terminal
(the build is marked `FAILURE`, mirroring the integrity-reject path) instead of
being silently left wedged in `dispatched`/`active`. A pack failure is
deterministic, so re-queueing would loop forever at the lane head.
