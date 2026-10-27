- **Starred looks and a photo's snapshots are no longer wiped after a failed read.** When the
  starred looks could not be read they showed as none, and the next star wrote that one star over
  the whole list; a photo's snapshots that could not be read (or had not been read yet) were
  replaced by the next snapshot taken. Both now write only after a successful read
  (`src/argentum/looks/looksStore.ts`, `src/argentum/snapshots/snapshotsStore.ts`, with tests).
