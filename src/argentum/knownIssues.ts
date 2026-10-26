/**
 * What is known to be broken, right now. Ours.
 *
 * WHY NOT READ THEM OUT OF THE CHANGELOG
 *
 * Because a changelog records what was true at a release, and this has to say
 * what is true today. The `### Known issues` list in `26.37.1` still claimed
 * lens auto-detection was broken long after `26.37.6` fixed it — a stale
 * warning is worse than none, because a reader trusts it and stops looking.
 *
 * So this is a list with one job, and the rule that comes with it: **when
 * something here is fixed, delete the entry in the same commit as the fix.**
 * The changelog is where it goes to be remembered.
 */

export interface KnownIssue {
  what: string;
  detail: string;
  /** True when it came from RapidRAW rather than from anything Argentum did. */
  inherited?: boolean;
}

export const KNOWN_ISSUES: KnownIssue[] = [
  {
    what: 'Kelvin readout is wrong on a Canon EOS-1D or 1Ds',
    detail:
      'These bodies keep their white balance where RapidRAW\x27s as-shot reader does not '
      + 'look, so in Kelvin mode the starting temperature shown is wrong. The picture '
      + 'is balanced correctly, and the picker and Auto still land neutral; only the '
      + 'number is off.',
  },
  {
    what: 'Zoom and pan feel sluggish',
    detail:
      'Noticeably behind darktable, and the mask overlay lags the image while '
      + 'moving. Not yet diagnosed — it could be the overlay, the render pipeline '
      + 'or the settings.',
    inherited: true,
  },
  {
    what: 'The screen conversion is Windows-only, and needs a matrix profile',
    detail:
      'The preview is converted for the screen it is on by reading the display '
      + 'profile Windows has for that monitor. On macOS and Linux nothing is read '
      + 'and nothing is converted, and the same is true of a monitor profile built '
      + 'as a lookup table rather than from three primaries. In both cases the '
      + 'picture is shown the way it was before any of this existed — which is '
      + 'right on an sRGB screen and over-saturated on a wide-gamut one.',
  },
  {
    what: 'Keeping metadata on a large TIFF needs a lot of memory',
    detail:
      'Writing the camera details into an exported TIFF rewrites the whole file '
      + 'through memory, so the export briefly needs three to four times the size '
      + 'of the file it is writing — around 2.5GB for a 60-megapixel 16-bit TIFF. '
      + 'On a machine short of memory that can make a large export slow or fail. '
      + 'Turning Keep metadata off skips it entirely, and the file is then written '
      + 'exactly as it was before.',
  },
  {
    what: 'Colour measured on two cameras',
    detail:
      'Colour is measured against the camera\x27s own JPEG on a Canon EOS 5D Mark II '
      + 'and an EOS R6 Mark III only. Ten public RAWs (Canon, Nikon, Sony, Fujifilm, '
      + 'Olympus, Pentax, a phone and a drone) are rendered in every build, which '
      + 'catches a change to how they look but does not say whether their colour '
      + 'is right.',
  },
];
