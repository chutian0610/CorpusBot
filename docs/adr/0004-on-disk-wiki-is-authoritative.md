# The on-disk Wiki is authoritative

Users may edit Wiki pages directly, including through Obsidian. Before ingest, query, or lint, the engine treats current files as authoritative and refreshes stored hashes and derived state rather than restoring pages from metadata. This keeps the local-folder and Obsidian workflow intact.
