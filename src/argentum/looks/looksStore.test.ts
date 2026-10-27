import { beforeEach, describe, expect, it, vi } from 'vitest';

const writes: unknown[] = [];
let favouritesRead: () => Promise<string[]>;

vi.mock('../ag', () => ({
  ag: vi.fn(async (name: string, args: { ids?: string[] }) => {
    if (name === 'builtin_presets') return [];
    if (name === 'looks_favourites') return favouritesRead();
    if (name === 'set_looks_favourites') writes.push(args.ids);
    return null;
  }),
}));
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => []) }));

const { useLooks } = await import('./looksStore');

describe('starred looks', () => {
  beforeEach(() => {
    writes.length = 0;
    useLooks.setState({ favourites: [], loaded: false });
  });

  it('are never written over when they could not be read', async () => {
    favouritesRead = () => Promise.reject(new Error('locked'));
    await useLooks.getState().load();
    useLooks.getState().toggleFavourite('fuji-classic-chrome');
    expect(writes).toEqual([]);
  });

  it('are written with the new star once read', async () => {
    favouritesRead = async () => ['kodak-portra'];
    await useLooks.getState().load();
    useLooks.getState().toggleFavourite('fuji-classic-chrome');
    expect(writes).toEqual([['kodak-portra', 'fuji-classic-chrome']]);
  });
});
