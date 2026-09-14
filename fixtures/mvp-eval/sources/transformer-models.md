# Transformer models

A transformer is a neural network architecture that processes tokens in parallel and uses attention to decide which other tokens are important while representing each token. Attention lets a model connect a pronoun to a distant noun, relate terms in a long document, and preserve context across a sequence.

Language transformers first split text into tokens, map each token to an embedding, and pass those embeddings through stacked attention and feed-forward layers. The usable amount of surrounding text is bounded by the model's context window. Output tokens are generated one at a time until the model produces a stopping boundary.
