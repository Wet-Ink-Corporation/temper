---
type: regex
target: { source: file, path: crates/ledger/data/accounts.v1 }
# H-16: the snapshot the settlement job and the support tool read stays a
# version 1 file with its three records unchanged.
pattern: '^# ledger accounts v1\n1001\|Ada Lovelace\|125000\n1002\|Grace Hopper\|-4250\n1003\|Edsger Dijkstra\|0\n?$'
---
