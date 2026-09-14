# Information retrieval

Information retrieval is the task of ranking documents by how well they satisfy an information need. A classical search engine splits documents into terms and builds an inverted index that maps each term to the documents containing it. Query time then intersects or unions those posting lists before scoring candidates.

BM25 is a bag-of-words ranking function that rewards documents containing rare query terms, increases with term frequency, and limits the effect of very long documents. Unlike a pure keyword match, BM25 considers how common a term is across the whole collection. It remains a strong baseline for lexical search.
