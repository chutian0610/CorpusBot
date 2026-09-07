# Page identity uses normalized exact matching

Existing pages are matched by template, page type, and canonical name after Unicode NFKC normalization, whitespace trimming and collapsing, and case folding. Aliases add resolvable keys but do not cause semantic or fuzzy merging. A near match with a different normalized name creates a new page and can produce a possible-duplicate warning, avoiding accidental merges of distinct concepts.
