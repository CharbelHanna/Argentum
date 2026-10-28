- **Five settings no control could change, and the code that read them.** "Require matching EXIF"
  and "Group edited files" for RAW+JPEG grouping (grouping now always works as their defaults did:
  same name, edited files included), and RapidRAW's three baseline exposure settings, whose only
  reader always answered 0 EV, so its whole chain (the auto-adjust and thumbnail calls included)
  did nothing. A `settings.json` that still holds any of the five keeps working; the keys are
  dropped at the next save. Also nine store fields and actions nothing used. No change in the app.
