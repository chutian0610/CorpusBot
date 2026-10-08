# Ingest extraction pipeline

## Goals

The pipeline turns one source into a provenance-linked set of wiki pages
without imposing a fixed limit on the number of entities or concepts. Output
size is controlled by batching and the configured LLM token budget, not by
discarding candidates.

## Raw provenance

The original Markdown is stored unchanged at
`raw/<sha256>/<original-name>`. Its bytes are the provenance anchor and are not
rewritten with generated metadata.

Ingest processing reads the body after any source-provided frontmatter. This
keeps YAML metadata from leaking into analysis and generation prompts. The
generated source page owns the link back to the raw file through frontmatter:

```yaml
raw:
  path: raw/<sha256>/<original-name>
  original_name: example.md
  sha256: <64-character lowercase sha256>
  size: 12345
```

## Candidate inventory

The analyze workflow produces a candidate inventory rather than a final page
count. Entities and concepts include a confidence score, importance
(`core`, `supporting`, or `incidental`), and evidence copied from the source.
There is no `max 6` page limit.

Rust normalizes and merges the inventory using page identities. Duplicate names
and aliases are merged, evidence is de-duplicated, and the more important
classification wins.

## Batched page generation

Candidates are ordered by importance and confidence, then split into batches of
four pages. Each request receives only source excerpts scored against that
batch, reducing prompt noise and avoiding one giant JSON response.

Each batch request asks for one unified `pages` array. Every item carries a
`page_type` of `entity` or `concept`, so the model does not need to choose
between two top-level arrays. Rust converts the pages back to typed entity and
concept drafts, merges all batch plans, de-duplicates page identities, and then
creates or updates pages in one atomic ingest commit.

The Rust normalizer also repairs the legacy mis-nested shape in which concepts
were wrapped inside the old `entities` array. A batch whose output still cannot
be normalized retries without discarding results from batches that already
passed schema parsing.

Core pages receive two or three substantive sections. Supporting pages receive
one or two. Incidental pages may be concise evidence-backed stubs, so a dense
source does not lose candidates merely because it contains many topics.

## Auditing

Each analysis and generation attempt persists request/response artifacts under
`.wiki-db/audit/<run-id>/`. Batch artifacts are named with a batch index.
Events include token usage, the configured output cap, finish reason, and a
`truncated` flag so output-limit failures are visible without inspecting the
provider response.

Runtime logs repeat the diagnosis at the batch boundary: the candidate JSON,
batch index and total, response artifact, provider/model, token usage, finish
reason, response preview, and parse error. This makes a rejected batch visible
without opening audit JSON by hand.
