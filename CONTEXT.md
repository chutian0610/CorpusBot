# CorpusBot

CorpusBot is a local knowledge compilation engine. It turns curated raw documents into a maintained Wiki and answers questions against that Wiki.

## Language

**Workspace**:
A local knowledge-base directory containing Raw Sources, Wiki Pages, and engine-owned state.
_Avoid_: Project, knowledge base, repository

**Raw Source**:
An immutable input document curated by the user and imported into the Workspace.
_Avoid_: Source file, uploaded file, corpus

**Source Version**:
The immutable content state of a Raw Source. A changed input file becomes a new Source Version rather than replacing the old one.
_Avoid_: File revision, upload, document

**Source Page**:
A Wiki Page that represents exactly one Source Version.
_Avoid_: Raw page, source file page

**Wiki Page**:
A Markdown page maintained in the Wiki. Its current on-disk content is authoritative.
_Avoid_: Note, document, cache entry

**Page Identity**:
The normalized page type and canonical name used to decide whether Ingest should update an existing page or create a new page.
_Avoid_: Filename, title, entity ID

**Draft**:
The complete proposed set of Wiki changes produced by one Ingest Run. It is not authoritative until committed.
_Avoid_: Suggestion, generated file, patch

**Ingest Run**:
One attempt to analyze a Source Version, produce a Draft, validate it, and commit it.
_Avoid_: Job, task, import

**Workflow Attempt**:
One bounded execution of a workflow node, including its validation outcome and transition decision.
_Avoid_: Retry, run, graph step

**Audit Event**:
The persisted record of a workflow or model attempt. It explains what happened without exposing credentials.
_Avoid_: Log line, trace, debug output

**Commit**:
The engine transition that makes a validated Draft authoritative in the Wiki.
_Avoid_: Save, write, publish

**Resource**:
A mutable artifact that an engine transaction can read or change, such as a Wiki Page, Index, Log, or derived store.
_Avoid_: File, page, table

**Resource Revision**:
The state marker used to detect whether a Resource has changed since it was read.
_Avoid_: Version number, timestamp, file lock

**Revision Manifest**:
The read set captured by an operation so its answer or report can be tied to one consistent set of Resource Revisions.
_Avoid_: Cache, snapshot, inventory

**Manifest ID**:
The stable identifier assigned to a Revision Manifest.
_Avoid_: Snapshot ID, timestamp, content hash

**Snapshot**:
An immutable point-in-time capture of the authoritative Workspace content used for history and recovery.
_Avoid_: Backup, checkpoint, commit

**Snapshot ID**:
The immutable identifier assigned to a Snapshot.
_Avoid_: Commit message, timestamp, branch name

**Workspace Lock**:
A guard that allows one Workspace mutation or recovery workflow to commit at a time. It does not govern external editors or read-only snapshots.
_Avoid_: Git lock, file lock, mutex

**Dirty Workspace**:
The state in which tracked Wiki or Raw content differs from the current Snapshot history. New Ingest is blocked in this state.
_Avoid_: Unsaved, broken, conflicting

**Ingest Guard**:
The preflight that blocks Ingest unless Recovery is complete and tracked Workspace content is clean.
_Avoid_: Lint, validation, queue check

**Recovery**:
The workflow that finishes or safely reverts an interrupted Ingest Run before other mutations may continue.
_Avoid_: Repair, rollback, restart

**Restore**:
An explicit, conflict-checked user action that returns Workspace content to a selected Snapshot after preserving the current state.
_Avoid_: Rollback, undo, reset

**User Edit**:
A human modification to a Wiki Page. It is authoritative and requires the stored page state to be refreshed.
_Avoid_: Conflict, override, manual change

**Citation**:
A query answer reference consisting of a Wiki Page and a verifiable supporting quote.
_Avoid_: Link, source, reference

**Lint**:
A deterministic health report about the Wiki. It reports problems; it does not authorize or perform repairs.
_Avoid_: Validation, test, audit

**Index**:
The generated catalog of Wiki Pages used for browsing, consistency checks, and retrieval.
_Avoid_: Table of contents, cache

**Log**:
The append-only record of committed Ingest Runs.
_Avoid_: History, changelog, debug log
