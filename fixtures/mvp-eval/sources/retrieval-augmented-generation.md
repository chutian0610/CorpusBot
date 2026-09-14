# Retrieval-augmented generation

Retrieval-augmented generation combines a retriever with a language model. The source document is split into chunks, each chunk is embedded, and the chunks related to a question are retrieved before the model writes an answer. Supplying this evidence reduces reliance on the model's memorized prior knowledge.

Good retrieval-augmented systems preserve enough surrounding text for a chunk to remain understandable, remove duplicate passages, and limit context to the most relevant evidence. A trustworthy answer should cite the retrieved passage that supports each claim. If no passage supports the question, the system should say that evidence is insufficient.
