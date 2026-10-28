- **The UI walk reports every command its stub does not answer.** Each page load started the list
  anew and the walk read it once per chapter, so 14 of 21 such commands went unreported. The stub
  now answers all 21 as the app's backend does: null for the 17 that return nothing or None, and a
  path, a size or a log file's location for the other four.
