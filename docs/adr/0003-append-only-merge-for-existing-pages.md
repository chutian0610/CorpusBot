# Ingest merges existing pages append-only

The MVP does not let an LLM rewrite an existing entity or concept page. The engine deterministically unions `tags`, `related`, and `sources`, while generated content is added in a section explicitly attributed to the new Source Version. Existing prose remains intact so a second import cannot erase earlier reasoning.
