# Ingest requires a clean tracked Workspace and interrupted applies recover automatically

Ingest mixes existing knowledge with a new Source Version, so it must not silently bundle unrelated human edits into the same commit. The Ingest Guard therefore rejects a new run when Recovery is pending, Git is in an unsafe state, or tracked `wiki/` and `raw/` content is dirty. The user first creates an explicit Snapshot; the workflow does not auto-snapshot unrelated edits merely to let Ingest proceed.

An interrupted apply is different: it is an incomplete engine transaction. On the next Workspace open, CorpusBot automatically runs Recovery before mutations. If the run's commit is already present, it marks the run committed; otherwise it restores only that run's touched paths from the apply baseline, after copying the current touched-path contents into engine-private recovery storage. New Ingest remains blocked until this workflow finishes.
