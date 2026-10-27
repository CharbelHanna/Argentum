import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Snapshot } from './snapshotsStore';

const writes: unknown[] = [];
let read: () => Promise<Snapshot[]>;

vi.mock('../ag', () => ({
  ag: vi.fn(async (name: string, args: { snapshots?: Snapshot[] }) => {
    if (name === 'snapshots') return read();
    if (name === 'set_snapshots') writes.push(args.snapshots);
    return null;
  }),
}));

const { useSnapshots } = await import('./snapshotsStore');

const snapshot = (id: string): Snapshot => ({ id, name: id, at: 0, adjustments: {} as Snapshot['adjustments'] });

describe("a photo's snapshots", () => {
  beforeEach(() => {
    writes.length = 0;
    useSnapshots.setState({ byPath: {} });
  });

  it('are never written over when they could not be read', async () => {
    read = () => Promise.reject(new Error('sidecar locked'));
    await useSnapshots.getState().load('IMG.CR2');
    useSnapshots.getState().keep('IMG.CR2', [snapshot('new')]);
    expect(writes).toEqual([]);
  });

  it('are written once read', async () => {
    read = async () => [snapshot('old')];
    await useSnapshots.getState().load('IMG.CR2');
    useSnapshots.getState().keep('IMG.CR2', [snapshot('old'), snapshot('new')]);
    expect(writes).toEqual([[snapshot('old'), snapshot('new')]]);
  });
});
