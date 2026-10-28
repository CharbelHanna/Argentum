- **The geometry part of every cache key is built in one place.** `hash_geometry`
  (`cache_utils.rs`) hashes the warp keys and the camera's built-in lens correction; the seven
  pasted copies of that loop and the nine places that folded the lens keys in afterwards are
  gone. The geometry and transform hashes now include the built-in lens correction themselves,
  so a mask that reads the warped picture is no longer reused from the other setting when the
  built-in correction is switched, and the export size
  estimate can reuse the preview's transformed picture again (it compared a hash without the
  lens keys against one with them, so it never matched). These keys live only in memory:
  nothing stored on disk changes.
