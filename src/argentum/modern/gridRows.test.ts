import { describe, expect, it } from 'vitest';
import { buildJustifiedRows } from '../../components/panel/library/LibraryGrid';
import { gridRowsOf, rowNeighbour } from './gridRows';

const photos = (count: number, prefix = 'p') => Array.from({ length: count }, (_, i) => ({ path: `${prefix}${i}` }));

/** LibraryGrid's square and original-shape layout: `columns` photos to a row. */
const uniform = (images: { path: string }[], columns: number) => {
  const rows = [];
  for (let i = 0; i < images.length; i += columns) rows.push({ type: 'images', images: images.slice(i, i + columns) });
  return rows;
};

const ITEM_PX = 200;
const GAP_PX = 12;

describe('Up and Down in the library grid', () => {
  // 10 photos, 4 to a row:  p0 p1 p2 p3 / p4 p5 p6 p7 / p8 p9
  const grid = gridRowsOf([...uniform(photos(10), 4), { type: 'footer' }], ITEM_PX, GAP_PX);

  it('keeps the column', () => {
    expect(rowNeighbour(grid, 'p1', 1)).toBe('p5');
    expect(rowNeighbour(grid, 'p5', 1)).toBe('p9');
    expect(rowNeighbour(grid, 'p6', -1)).toBe('p2');
  });

  it('goes to the last photo when the last row is shorter', () => {
    expect(rowNeighbour(grid, 'p7', 1)).toBe('p9');
    expect(rowNeighbour(grid, 'p6', 1)).toBe('p9');
  });

  it('stays put on the top and bottom rows', () => {
    expect(rowNeighbour(grid, 'p2', -1)).toBe('p2');
    expect(rowNeighbour(grid, 'p8', 1)).toBe('p8');
  });

  it('follows the column count the grid was laid out with', () => {
    const narrow = gridRowsOf(uniform(photos(10), 3), ITEM_PX, GAP_PX);
    expect(rowNeighbour(narrow, 'p1', 1)).toBe('p4');
    const single = gridRowsOf(uniform(photos(3), 1), ITEM_PX, GAP_PX);
    expect(rowNeighbour(single, 'p1', 1)).toBe('p2');
    expect(rowNeighbour(single, 'p1', -1)).toBe('p0');
  });

  it('picks the nearest photo across in justified rows of different lengths', () => {
    // Row 0: a square, then a panorama; row 1: three portraits.
    const ratios = { a: 1, b: 3, c: 0.66, d: 0.66, e: 0.66 };
    const images = Object.keys(ratios).map((path) => ({ path }));
    const layout = buildJustifiedRows(images, ratios, 800, 200, GAP_PX);
    expect(layout.map((row) => row.images.map((image: { path: string }) => image.path))).toEqual([
      ['a', 'b'],
      ['c', 'd', 'e'],
    ]);
    const rows = gridRowsOf(layout, ITEM_PX, GAP_PX);
    expect(rowNeighbour(rows, 'a', 1)).toBe('c');
    expect(rowNeighbour(rows, 'b', 1)).toBe('e');
    // The second portrait sits under the square, not under the row's second photo.
    expect(rowNeighbour(rows, 'd', -1)).toBe('a');
    expect(rowNeighbour(rows, 'e', -1)).toBe('b');
  });

  it('moves between folder groups in the Subfolders view, skipping their headings', () => {
    const layout = [
      { type: 'header', path: 'one' },
      ...uniform(photos(5, 'a'), 4),
      { type: 'header', path: 'two' },
      ...uniform(photos(3, 'b'), 4),
      { type: 'footer' },
    ];
    const rows = gridRowsOf(layout, ITEM_PX, GAP_PX);
    expect(rowNeighbour(rows, 'a4', 1)).toBe('b0');
    expect(rowNeighbour(rows, 'b0', -1)).toBe('a4');
    expect(rowNeighbour(rows, 'b2', -1)).toBe('a4');
    expect(rowNeighbour(rows, 'a2', 1)).toBe('a4');
  });

  it('says when the grid does not show the photo', () => {
    expect(rowNeighbour(grid, 'elsewhere', 1)).toBeNull();
    expect(rowNeighbour([], 'p0', 1)).toBeNull();
  });
});
