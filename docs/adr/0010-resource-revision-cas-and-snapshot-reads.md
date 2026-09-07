# Resource Revision CAS with a short commit lock

LLM generation is long, while the final apply must be short and safe. A Draft therefore records the expected Resource Revision for every mutable Resource it touches. Before Commit, the engine checks all expected revisions; any mismatch rejects the whole Draft instead of producing a partial write. A short Workspace Lock serializes only the final Commit, Recovery, Snapshot, and Restore workflow.

Read-only operations do not take this lock. They first reject an interrupted run that still needs Recovery, because a partial transaction is not a valid read basis. Once Recovery is complete, Query and Lint capture a Revision Manifest at startup, read the recorded revisions, and label their results with that manifest. If content changes during the operation, the earlier report remains attributable to its manifest instead of silently mixing versions.
