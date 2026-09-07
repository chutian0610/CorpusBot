# Use a small workflow kernel instead of a graph framework

CorpusBot's business flow depends on LLM output, but the MVP graph shape is small and deterministic. We will use Rig only as the provider/model adapter and implement a typed workflow kernel in `corpusbot-agent`: named nodes, typed state, bounded attempts, deterministic transitions, and persisted audit events. The LLM may propose content, but Rust validators and workflow policy decide retries, repair, acceptance, and rejection.

We will not adopt `graph-flow`, the Rust LangGraph ports, or a Python LangGraph sidecar in the MVP. They add orchestration or persistence abstractions before the product needs them, and none of them can own CorpusBot's domain-level audit requirements. If Alpha's Deep Research needs richer branching or human-in-the-loop pauses, we can evaluate `graph-flow` separately, but the core commit path must remain controlled by our workflow kernel.
