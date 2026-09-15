# Git version control

Git records project states as commits. Each commit points to a complete snapshot of tracked content and stores metadata such as the author, timestamp, message, and parent commit. The commit identifier is a cryptographic hash of the commit object, so changing its content or history produces a different identifier.

A branch is a movable pointer to a commit rather than a copy of the files. Creating a commit advances the current branch to the new snapshot. Because every commit remembers its parent, Git can display history, compare versions, and restore a previous state without losing the path that produced the current content.
