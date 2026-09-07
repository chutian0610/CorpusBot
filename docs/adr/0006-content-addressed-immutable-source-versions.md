# Modified sources become immutable new versions

The MVP identifies a source by content hash. Reimporting identical content is a no-op, while changed content creates a separate immutable Source Version and Source Page. The MVP does not create version chains, replacements, or retractions; those relationships are deferred to Alpha so the first implementation can preserve provenance without resolving claim succession.
