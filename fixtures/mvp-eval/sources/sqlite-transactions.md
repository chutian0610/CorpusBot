# SQLite transactions

SQLite is an embedded relational database that stores a complete database in one file. A transaction groups reads and writes into an all-or-nothing unit. If any statement fails before commit, the database is not left with part of the transaction applied.

SQLite provides ACID properties: atomicity, consistency, isolation, and durability. In write-ahead logging mode, changes are first recorded in a WAL file, readers can continue while a writer prepares a transaction, and the WAL is checkpointed back into the main database later. This design improves concurrency while preserving recovery after a process crash.
