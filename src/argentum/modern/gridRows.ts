/**
 * Up and Down in the library grid: one row up or down, to the photo nearest
 * the same place across. Ours, from RapidRAW ecf7730f.
 *
 * The grid publishes its rows as it lays them out (LibraryGrid's `gridData`),
 * so the column count, justified rows of any length and the folder groups of
 * the Subfolders view all come from the one place that computes them; nothing
 * measures the grid a second time. A plain index-and-columns step would be
 * wrong in two of those three layouts: justified rows differ in length, and
 * each folder group starts a new row.
 */

/** A photo's place in a grid row: the centre of its cell, across, from the row's left edge. */
export interface GridCell {
  path: string;
  centerPx: number;
}

/** The grid's photos, row by row, top to bottom; folder headings left out. */
export type GridRows = GridCell[][];

/** A row of LibraryGrid's layout (`gridData.rows`), as far as the keyboard needs it. */
interface LayoutRow {
  type: string;
  images?: { path: string }[];
  /** Each cell's width in a justified row; absent when every cell is the grid's item width. */
  justifiedWidths?: number[];
}

/** The rows of photos in a laid-out grid, with each cell's centre across. */
export function gridRowsOf(layout: readonly LayoutRow[], itemWidthPx: number, gapPx: number): GridRows {
  // RapidRAW ecf7730f: src/components/panel/library/LibraryGrid.tsx
  return layout.flatMap((row) => {
    if (row.type !== 'images' || !row.images) return [];
    let leftPx = 0;
    const cells = row.images.map((image, i) => {
      const widthPx = row.justifiedWidths ? row.justifiedWidths[i] : itemWidthPx;
      const cell = { path: image.path, centerPx: leftPx + widthPx / 2 };
      leftPx += widthPx + gapPx;
      return cell;
    });
    return [cells];
  });
}

/**
 * Where Up (-1) or Down (+1) goes from `path`: the photo in the next row whose
 * centre is nearest; `path` itself on the top or bottom row; null when the
 * grid does not show `path` (a collapsed folder, or no grid on screen).
 */
export function rowNeighbour(rows: GridRows, path: string, step: -1 | 1): string | null {
  // RapidRAW ecf7730f: src/hooks/useKeyboardShortcuts.ts
  for (let r = 0; r < rows.length; r++) {
    const from = rows[r].find((cell) => cell.path === path);
    if (!from) continue;
    const target = rows[r + step];
    if (!target || target.length === 0) return path;
    const nearest = target.reduce((best, cell) =>
      Math.abs(cell.centerPx - from.centerPx) < Math.abs(best.centerPx - from.centerPx) ? cell : best,
    );
    return nearest.path;
  }
  return null;
}
