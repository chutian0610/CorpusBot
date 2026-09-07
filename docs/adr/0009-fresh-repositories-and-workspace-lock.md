# Workspaces use fresh repositories and a single Workspace Lock

A Workspace is always initialized as a new dedicated Git repository; CorpusBot does not adopt or reuse an existing repository, even though a Workspace may physically live inside an enclosing repository. History operations commit only CorpusBot's declared scope so unrelated staged files are never captured. A Workspace Lock serializes local mutations and recovery because Git and SQLite locks cannot express the higher-level invariant that one Ingest, Snapshot, Restore, or Recovery owns the Workspace at a time.
