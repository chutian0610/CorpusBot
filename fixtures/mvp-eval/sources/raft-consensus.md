# Raft consensus

Raft is a consensus algorithm used by replicated systems to maintain the same ordered log on every node. One server is the leader, while the remaining servers are followers. A candidate wins an election after receiving votes from a majority of the cluster. The winning leader coordinates all client writes.

The leader appends each client command to its local log and replicates that entry to followers. An entry becomes committed when it is stored on a majority. Committed entries are then applied to the state machine in log order. If a leader fails, a follower with an up-to-date log can become the new leader.
