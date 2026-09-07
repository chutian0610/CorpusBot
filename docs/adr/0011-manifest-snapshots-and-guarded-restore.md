# Snapshots are manifest-addressed and Restore checks conflicts

Snapshot and Restore must remain meaningful while users continue editing through Obsidian or an external Git client. A Snapshot is therefore materialized from a captured Revision Manifest and its recorded content, not from a moving mixture of files read at different moments. Its Manifest ID is derived from the sorted `(resource_path, resource_revision)` set, so identical read sets have identical IDs. If the Workspace changes after capture, the completed Snapshot remains a valid point-in-time and is marked `workspace_changed_after_capture`.

Restore prepares the selected Snapshot in engine-private storage and creates a pre-restore Snapshot first. Before switching the Workspace, it rechecks the current Revision Manifest; if it changed, Restore returns `RESTORE_CONFLICT` rather than overwriting the new edits. The final replacement is short and journaled so an interrupted Restore can be retried safely.
