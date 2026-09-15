# Vector databases

A vector database stores embeddings, which are dense numeric representations of text, images, or audio. Similar items produce nearby vectors, so the database can answer similarity search by computing distances between a query vector and stored vectors.

Exact nearest-neighbor search is expensive at scale. Many vector indexes use approximate nearest-neighbor methods such as hierarchical navigable small world graphs. These indexes trade a small amount of recall for much lower query latency. A vector search result normally returns the original item, its stored metadata, and a similarity score.
