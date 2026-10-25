// The look of a guide drawn over the photo: a white line on a dark edge twice
// its width, so it reads on a bright sky and in shadow alike. The edge is drawn
// first, the line on top. The values are the white-balance sample outline's,
// which drew this pair before the crop guides did.
// The idea, not code: darktable src/gui/guides.c (dt_guides_draw) draws a dark
// line under its light guides, and RAWmakase v0.4.3 src/app/viewport.rs does
// the same for its straighten ruler.

export interface GuideStroke {
  readonly color: string;
  readonly widthPx: number;
}

export const GUIDE_EDGE: GuideStroke = { color: 'rgba(0, 0, 0, 0.6)', widthPx: 3 };
export const GUIDE_LINE: GuideStroke = { color: '#ffffff', widthPx: 1.5 };
