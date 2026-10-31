import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { toast } from 'react-toastify';
import type { LookProgress, LooksDeleted, ProviderLook, ProviderLooks } from '../ag.generated';

const PREVIEW_JOB = 9;

/** Every `ag` call, in order. */
const calls: { name: string; args: unknown }[] = [];
const invoked: { name: string; args: unknown }[] = [];
let answerOffer: ProviderLooks | null = null;
/** None for a refusal. */
let answerDeleted: LooksDeleted | null = null;
let fileThere = false;
let hear: ((event: { payload: LookProgress }) => void) | null = null;
const listened = vi.fn();
const sidecars: Record<string, Record<string, unknown>> = {};

vi.mock('../ag', () => ({
  ag: vi.fn(async (name: string, args: unknown) => {
    calls.push({ name, args });
    if (name === 'provider_looks') return answerOffer;
    if (name === 'provider_look_make') return 7;
    if (name === 'provider_look_preview') return PREVIEW_JOB;
    if (name === 'provider_look_file') return fileThere;
    if (name === 'provider_look_delete') {
      if (!answerDeleted) throw new Error('refused');
      return answerDeleted;
    }
    return null;
  }),
}));
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (name: string, args: { path?: string }) => {
    invoked.push({ name, args });
    if (name === 'load_metadata') return { adjustments: sidecars[args.path ?? ''] ?? {} };
    return null;
  }),
}));
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (_name: string, handler: (event: { payload: LookProgress }) => void) => {
    listened();
    hear = handler;
    return () => {};
  }),
}));
vi.mock('react-toastify', () => ({ toast: { success: vi.fn(), info: vi.fn(), error: vi.fn() } }));
vi.mock('../locales', () => ({ agText: (key: string) => key }));

const { useProviderLooks, regularLooksAreOff, takeFromPaste, lookFor, previewsFor, restoreLook, restoreLooks } =
  await import('./providerLooksStore');
const { useEditorStore } = await import('../../store/useEditorStore');
const { PROVIDER_LOOK_PATH } = await import('./providerLook');

const OFFER: ProviderLooks = {
  cameras: [
    { id: 'cam1', name: 'Camera One', make: 'Acme', model: 'One' },
    { id: 'cam2', name: 'Camera Two', make: 'Acme', model: 'Two' },
  ],
  looks: [
    { id: 'look1', name: 'Invented Stock', group: null },
    { id: 'look2', name: 'Other Stock', group: null },
  ],
  options: [],
  on: true,
  photo: { camera: 'cam1', files: [], previews: [], beside: 0 },
};

const OPTION = { id: 'optionA', label: 'Invented choice', default: false };
const WITH_OPTION: ProviderLooks = { ...OFFER, options: [OPTION] };

const LOOK = OFFER.looks[0];
const OPEN = 'D:/photos/open.cr3';
const OTHER = 'D:/photos/other.cr3';

const progress = (
  path: string,
  outcome: LookProgress['outcome'],
  look = 'look1',
  options: LookProgress['options'] = {},
): LookProgress => ({
  jobId: 7,
  path,
  camera: 'cam1',
  look,
  options,
  outcome,
  preview: false,
});

/** Lets the promises in flight settle. */
const settled = () => new Promise((resolve) => setTimeout(resolve, 0));

const namesCalled = () => calls.map((c) => c.name);

async function offered(offer: ProviderLooks | null = OFFER) {
  answerOffer = offer;
  await useProviderLooks.getState().load(OPEN);
}

function openPhoto(adjustments: Record<string, unknown> = {}) {
  useEditorStore.setState({
    selectedImage: { path: OPEN } as never,
    adjustments: { rawToneRendering: 'dngStandard', ...adjustments } as never,
  });
}

beforeEach(() => {
  calls.length = 0;
  invoked.length = 0;
  listened.mockClear();
  useProviderLooks.getState().reset();
  fileThere = false;
  openPhoto();
});

afterEach(() => {
  vi.useRealTimers();
});

describe('with no provider', () => {
  it('nothing is offered and nothing listens', async () => {
    await offered(null);
    expect(useProviderLooks.getState().offer).toBeNull();
    expect(listened).not.toHaveBeenCalled();
  });

  it('regular looks stay on', async () => {
    await offered(null);
    expect(regularLooksAreOff(useProviderLooks.getState())).toBe(false);
  });

  it('a paste is left as it is', async () => {
    await offered(null);
    const changes = { lutPath: PROVIDER_LOOK_PATH, agProviderLook: { camera: 'cam1', look: 'look1', before: {} } };
    expect(takeFromPaste([OTHER], changes)).toBe(changes);
    expect(namesCalled()).not.toContain('provider_look_make');
  });
});

describe('the switch', () => {
  it('turns regular looks off for a photo that shows provider looks', async () => {
    await offered();
    expect(regularLooksAreOff(useProviderLooks.getState())).toBe(true);
  });

  it('leaves them on while it is off', async () => {
    await offered({ ...OFFER, on: false });
    expect(regularLooksAreOff(useProviderLooks.getState())).toBe(false);
  });

  it('leaves them on for a photo with no provider looks, even when on', async () => {
    await offered({ ...OFFER, photo: null });
    expect(regularLooksAreOff(useProviderLooks.getState())).toBe(false);
  });

  it('is saved, and cancels what is waiting', async () => {
    await offered();
    await useProviderLooks.getState().pick(OPEN, LOOK, 'cam1', {});
    await useProviderLooks.getState().setOn(false);
    expect(calls).toContainEqual({ name: 'provider_looks_switch', args: { on: false } });
    expect(calls).toContainEqual({ name: 'provider_look_cancel', args: { jobId: 7 } });
    expect(useProviderLooks.getState().waiting[OPEN]).toBeUndefined();
    expect(useProviderLooks.getState().offer?.on).toBe(false);
  });

  it('never touches a look already on the photo', async () => {
    const on = { lutPath: PROVIDER_LOOK_PATH, agProviderLook: { camera: 'cam1', look: 'look1', before: {} } };
    openPhoto(on);
    await offered();
    await useProviderLooks.getState().setOn(false);
    expect(useEditorStore.getState().adjustments.lutPath).toBe(PROVIDER_LOOK_PATH);
  });
});

describe('picking a look', () => {
  beforeEach(() => offered());

  it('asks for the file and leaves the photo as it is', async () => {
    await useProviderLooks.getState().pick(OPEN, LOOK, 'cam1', {});
    expect(calls).toContainEqual({
      name: 'provider_look_make',
      args: { paths: [OPEN], camera: 'cam1', look: 'look1', options: {}, remake: false, front: true },
    });
    expect(useEditorStore.getState().adjustments.lutPath).toBeUndefined();
  });

  it('shows the spinner only after 300 ms', async () => {
    vi.useFakeTimers();
    const asking = useProviderLooks.getState().pick(OPEN, LOOK, 'cam1', {});
    await vi.advanceTimersByTimeAsync(299);
    await asking;
    expect(useProviderLooks.getState().waiting[OPEN].spinning).toBe(false);
    await vi.advanceTimersByTimeAsync(2);
    expect(useProviderLooks.getState().waiting[OPEN].spinning).toBe(true);
  });

  it('puts Linear and the LUT on in one step when the file is ready', async () => {
    await useProviderLooks.getState().pick(OPEN, LOOK, 'cam1', {});
    hear?.({ payload: progress(OPEN, 'done') });
    await settled();
    const { adjustments } = useEditorStore.getState();
    expect(adjustments).toMatchObject({
      lutPath: PROVIDER_LOOK_PATH,
      lutName: 'Invented Stock',
      lutSize: 64,
      lutIntensity: 100,
      lutIsSceneReferred: false,
      agLutDeclaredInput: true,
      rawToneRendering: 'linear',
      rawToneCurve: null,
      rawToneCurveMode: null,
      agProviderLook: {
        camera: 'cam1',
        look: 'look1',
        before: { rawToneRendering: 'dngStandard', rawToneCurve: null, rawToneCurveMode: null },
      },
    });
    expect(useProviderLooks.getState().waiting[OPEN]).toBeUndefined();
  });

  it('goes on another photo through its own file if that one was left', async () => {
    sidecars[OTHER] = { rawToneRendering: 'baseCurve' };
    await useProviderLooks.getState().pick(OTHER, LOOK, 'cam1', {});
    hear?.({ payload: progress(OTHER, 'done') });
    await settled();
    const put = invoked.find((c) => c.name === 'apply_adjustments_to_paths')?.args as {
      paths: string[];
      adjustments: { agProviderLook: ProviderLook };
    };
    expect(put.paths).toEqual([OTHER]);
    expect(put.adjustments.agProviderLook.before.rawToneRendering).toBe('baseCurve');
    expect(useEditorStore.getState().adjustments.lutPath).toBeUndefined();
  });

  it('says so on the row when it could not be made, and the photo stays', async () => {
    await useProviderLooks.getState().pick(OPEN, LOOK, 'cam1', {});
    hear?.({ payload: progress(OPEN, 'failed') });
    await settled();
    expect(useProviderLooks.getState().note).toEqual({ photo: OPEN, look: 'look1', text: 'looksProviderFailed' });
    expect(useEditorStore.getState().adjustments.lutPath).toBeUndefined();
  });

  it('says a folder cannot be written', async () => {
    await useProviderLooks.getState().pick(OPEN, LOOK, 'cam1', {});
    hear?.({ payload: progress(OPEN, 'folder') });
    await settled();
    expect(useProviderLooks.getState().note?.text).toBe('looksProviderFolder');
  });

  it('the latest pick wins: the one it replaced reports nothing', async () => {
    await useProviderLooks.getState().pick(OPEN, LOOK, 'cam1', {});
    await useProviderLooks.getState().pick(OPEN, OFFER.looks[1], 'cam1', {});
    hear?.({ payload: progress(OPEN, 'cancelled') });
    hear?.({ payload: progress(OPEN, 'done') });
    await settled();
    expect(useEditorStore.getState().adjustments.lutPath).toBeUndefined();
    expect(useProviderLooks.getState().waiting[OPEN]?.look).toBe('look2');
  });

  it('Esc or a regular LUT cancels it', async () => {
    await useProviderLooks.getState().pick(OPEN, LOOK, 'cam1', {});
    await useProviderLooks.getState().cancel(OPEN);
    expect(calls).toContainEqual({ name: 'provider_look_cancel', args: { jobId: 7 } });
    expect(useProviderLooks.getState().waiting[OPEN]).toBeUndefined();
  });
});

describe('the camera picker', () => {
  beforeEach(() => offered());

  it('starts on the photo’s own camera', () => {
    expect(useProviderLooks.getState().cameraOf(OPEN, {})).toBe('cam1');
  });

  it('starts on the camera the edit names', () => {
    const edit = { lutPath: PROVIDER_LOOK_PATH, agProviderLook: { camera: 'cam2', look: 'look1', before: {} } };
    expect(useProviderLooks.getState().cameraOf(OPEN, edit)).toBe('cam2');
  });

  it('keeps a hand choice for that photo only', async () => {
    await useProviderLooks.getState().chooseCamera(OPEN, 'cam2', {});
    expect(useProviderLooks.getState().cameraOf(OPEN, {})).toBe('cam2');
    expect(useProviderLooks.getState().cameraOf(OTHER, {})).toBe('cam1');
  });

  it('cancels a look still being made', async () => {
    await useProviderLooks.getState().pick(OPEN, LOOK, 'cam1', {});
    await useProviderLooks.getState().chooseCamera(OPEN, 'cam2', {});
    expect(calls).toContainEqual({ name: 'provider_look_cancel', args: { jobId: 7 } });
    expect(useProviderLooks.getState().waiting[OPEN]).toBeUndefined();
  });

  it('with no look on only changes what a click makes', async () => {
    await useProviderLooks.getState().chooseCamera(OPEN, 'cam2', {});
    expect(namesCalled()).not.toContain('provider_look_make');
  });

  it('with a look on makes the same look for the new camera', async () => {
    const edit = { lutPath: PROVIDER_LOOK_PATH, agProviderLook: { camera: 'cam1', look: 'look2', before: {} } };
    await useProviderLooks.getState().chooseCamera(OPEN, 'cam2', edit);
    expect(calls).toContainEqual({
      name: 'provider_look_make',
      args: { paths: [OPEN], camera: 'cam2', look: 'look2', options: {}, remake: false, front: true },
    });
  });
});

describe('an option', () => {
  const LOOK_ON = { camera: 'cam1', look: 'look1', before: {} };
  const onPhoto = (options?: Record<string, boolean>) => ({
    lutPath: PROVIDER_LOOK_PATH,
    agProviderLook: options ? { ...LOOK_ON, options } : LOOK_ON,
  });
  const makes = () => calls.filter((c) => c.name === 'provider_look_make').map((c) => c.args);
  const choose = (value: boolean, edit: Record<string, unknown> = {}) =>
    useProviderLooks.getState().chooseOption(OPEN, OPTION, value, edit);
  const optionsOf = (edit: Record<string, unknown> = {}, photo = OPEN) =>
    useProviderLooks.getState().optionsOf(photo, edit);

  beforeEach(() => offered(WITH_OPTION));

  it('starts at its default, for an edit saved before options existed too', () => {
    expect(optionsOf()).toEqual({});
    expect(optionsOf(onPhoto())).toEqual({});
  });

  it('starts from the edit', () => {
    expect(optionsOf(onPhoto({ optionA: true }))).toEqual({ optionA: true });
  });

  it('the provider no longer offers does not show or travel, and its look stays', () => {
    expect(optionsOf(onPhoto({ gone: true }))).toEqual({});
  });

  it('keeps a hand choice for that photo only', async () => {
    await choose(true);
    expect(optionsOf()).toEqual({ optionA: true });
    expect(optionsOf({}, OTHER)).toEqual({});
  });

  it('with no look on only changes what a click makes', async () => {
    await choose(true);
    expect(makes()).toEqual([]);
  });

  it('with a look on remakes that photo’s LUT and keeps the look until it is ready', async () => {
    openPhoto(onPhoto());
    const before = useEditorStore.getState().adjustments;
    await choose(true, onPhoto());
    expect(makes()).toEqual([
      { paths: [OPEN], camera: 'cam1', look: 'look1', options: { optionA: true }, remake: false, front: true },
    ]);
    expect(useEditorStore.getState().adjustments).toBe(before);

    hear?.({ payload: progress(OPEN, 'done', 'look1', { optionA: true }) });
    await settled();
    expect(useEditorStore.getState().adjustments.agProviderLook?.options).toEqual({ optionA: true });
  });

  it('back at its default asks for today’s file', async () => {
    await choose(false, onPhoto({ optionA: true }));
    expect(makes()).toEqual([
      { paths: [OPEN], camera: 'cam1', look: 'look1', options: {}, remake: false, front: true },
    ]);
  });

  it('remakes for the camera in the picker', async () => {
    await useProviderLooks.getState().chooseCamera(OPEN, 'cam2', {});
    calls.length = 0;
    await choose(true, onPhoto());
    expect(makes()).toMatchObject([{ camera: 'cam2' }]);
  });

  it('goes with a camera change', async () => {
    await choose(true);
    await useProviderLooks.getState().chooseCamera(OPEN, 'cam2', onPhoto());
    expect(makes()).toMatchObject([{ camera: 'cam2', options: { optionA: true } }]);
  });

  it('cancels a look still being made', async () => {
    await useProviderLooks.getState().pick(OPEN, LOOK, 'cam1', {});
    await choose(true);
    expect(calls).toContainEqual({ name: 'provider_look_cancel', args: { jobId: 7 } });
    expect(useProviderLooks.getState().waiting[OPEN]).toBeUndefined();
  });

  it('changed back and forth: the cancelled report of the old one leaves the new one waiting', async () => {
    await choose(true, onPhoto());
    await choose(false, onPhoto());
    hear?.({ payload: progress(OPEN, 'cancelled', 'look1', { optionA: true }) });
    await settled();
    expect(useProviderLooks.getState().waiting[OPEN]?.options).toEqual({});
  });

  it('a report from an earlier job of the same look and options is not this one’s', async () => {
    await choose(true, onPhoto());
    hear?.({ payload: { ...progress(OPEN, 'cancelled', 'look1', { optionA: true }), jobId: 6 } });
    await settled();
    expect(useProviderLooks.getState().waiting[OPEN]?.options).toEqual({ optionA: true });
  });

  it('a file made for other options never goes on', async () => {
    await useProviderLooks.getState().pick(OPEN, LOOK, 'cam1', { optionA: true });
    hear?.({ payload: progress(OPEN, 'done', 'look1', {}) });
    await settled();
    expect(useEditorStore.getState().adjustments.lutPath).toBeUndefined();
  });

  it('once on follows the edit, so an Undo shows on the checkbox', async () => {
    openPhoto(onPhoto());
    await choose(true, onPhoto());
    hear?.({ payload: progress(OPEN, 'done', 'look1', { optionA: true }) });
    await settled();
    expect(optionsOf(onPhoto())).toEqual({});
  });

  it('that could not be made leaves the look on the photo', async () => {
    openPhoto(onPhoto());
    await choose(true, onPhoto());
    hear?.({ payload: progress(OPEN, 'failed', 'look1', { optionA: true }) });
    await settled();
    expect(useProviderLooks.getState().note?.text).toBe('looksProviderFailed');
    expect(useEditorStore.getState().adjustments.agProviderLook).toEqual(LOOK_ON);
  });

  it('goes to a batch, each photo with the same', async () => {
    await useProviderLooks.getState().putOnSelection([OTHER], LOOK, null, { optionA: true });
    expect(makes()).toMatchObject([{ paths: [OTHER], options: { optionA: true } }]);
    hear?.({ payload: progress(OTHER, 'done', 'look1', { optionA: true }) });
    await settled();
    const put = invoked.find((c) => c.name === 'apply_adjustments_to_paths')?.args as {
      adjustments: { agProviderLook: ProviderLook };
    };
    expect(put.adjustments.agProviderLook.options).toEqual({ optionA: true });
  });

  it('asks for the look files of what is chosen', async () => {
    await useProviderLooks.getState().load(OPEN, { optionA: true });
    expect(calls).toContainEqual({ name: 'provider_looks', args: { path: OPEN, options: { optionA: true } } });
  });

  it('is pasted with the look', async () => {
    const pasted = { ...onPhoto({ optionA: true }), exposure: 1 };
    expect(takeFromPaste([OTHER], pasted)).toEqual({ exposure: 1 });
    await settled();
    expect(makes()).toMatchObject([{ paths: [OTHER], options: { optionA: true } }]);
  });

  it('no longer offered is left out of a paste, and the look still comes', async () => {
    takeFromPaste([OTHER], onPhoto({ gone: true }));
    await settled();
    expect(makes()).toMatchObject([{ paths: [OTHER], look: 'look1', options: {} }]);
  });

  it('asks for a missing file with the edit’s options, once for each set', async () => {
    await restoreLook(OPEN, onPhoto({ optionA: true }));
    await restoreLook(OPEN, onPhoto({ optionA: true }));
    await restoreLook(OPEN, onPhoto());
    expect(makes()).toMatchObject([{ options: { optionA: true } }, { options: {} }]);
  });

  it('asks again if a file that was there is gone later', async () => {
    fileThere = true;
    await restoreLook(OPEN, onPhoto({ optionA: true }));
    fileThere = false;
    await restoreLook(OPEN, onPhoto({ optionA: true }));
    expect(makes()).toMatchObject([{ options: { optionA: true } }]);
  });

  it('looks for the file of the edit’s options', async () => {
    fileThere = true;
    await restoreLook(OPEN, onPhoto({ optionA: true }));
    expect(calls).toContainEqual({
      name: 'provider_look_file',
      args: { path: OPEN, camera: 'cam1', look: 'look1', options: { optionA: true } },
    });
    expect(makes()).toEqual([]);
  });

  it('no longer offered: its file is not asked for', async () => {
    await restoreLook(OPEN, onPhoto({ gone: true }));
    restoreLooks([OTHER], onPhoto({ gone: true }));
    await settled();
    expect(makes()).toEqual([]);
  });

  it('a preset brings to photos in the grid goes with its look', async () => {
    restoreLooks([OTHER], onPhoto({ optionA: true }));
    await settled();
    expect(makes()).toMatchObject([{ paths: [OTHER], options: { optionA: true } }]);
  });

  it('the file arriving for a look already on leaves the edit alone', async () => {
    const edit = onPhoto({ optionA: true });
    await restoreLook(OPEN, edit);
    openPhoto(edit);
    hear?.({ payload: progress(OPEN, 'done', 'look1', { optionA: true }) });
    await settled();
    expect(useEditorStore.getState().adjustments.agProviderLook).toEqual(edit.agProviderLook);
  });
});

describe('a provider that offers no options', () => {
  beforeEach(() => offered());

  it('has none for any photo, and sends none', async () => {
    expect(useProviderLooks.getState().optionsOf(OPEN, {})).toEqual({});
    await useProviderLooks.getState().pick(OPEN, LOOK, 'cam1', {});
    hear?.({ payload: progress(OPEN, 'done') });
    await settled();
    expect(useEditorStore.getState().adjustments.agProviderLook).not.toHaveProperty('options');
  });
});

describe('a batch', () => {
  beforeEach(() => offered());

  it('counts as each photo finishes, and its Undo removes what went on', async () => {
    sidecars[OTHER] = { rawToneRendering: 'baseCurve' };
    await useProviderLooks.getState().putOnSelection([OTHER, 'D:/photos/third.cr3'], LOOK, null, {});
    expect(calls).toContainEqual({
      name: 'provider_look_make',
      args: {
        paths: [OTHER, 'D:/photos/third.cr3'],
        camera: null,
        look: 'look1',
        options: {},
        remake: false,
        front: false,
      },
    });
    hear?.({ payload: progress(OTHER, 'done') });
    await settled();
    expect(useProviderLooks.getState().batch).toMatchObject({ done: 1, total: 2 });

    await useProviderLooks.getState().cancelBatch();
    expect(calls).toContainEqual({ name: 'provider_look_cancel', args: { jobId: 7 } });
    expect(useProviderLooks.getState().batch).toBeNull();

    invoked.length = 0;
    await useProviderLooks.getState().undoLast();
    expect(invoked.find((c) => c.name === 'apply_adjustments_to_paths')?.args).toMatchObject({
      paths: [OTHER],
      adjustments: { rawToneRendering: 'baseCurve', lutPath: null },
    });
  });
});

describe('a batch with the photo the app has open', () => {
  beforeEach(() => offered());

  it('is undone there too', async () => {
    await useProviderLooks.getState().putOnSelection([OPEN], LOOK, null, {});
    hear?.({ payload: progress(OPEN, 'done') });
    await settled();
    expect(useEditorStore.getState().adjustments.lutPath).toBe(PROVIDER_LOOK_PATH);

    await useProviderLooks.getState().undoLast();
    const { adjustments } = useEditorStore.getState();
    expect(adjustments.lutPath ?? null).toBeNull();
    expect(adjustments.rawToneRendering).toBe('dngStandard');
    expect(adjustments.agProviderLook ?? null).toBeNull();
  });
});

describe('a paste', () => {
  const pasted = {
    exposure: 1,
    lutPath: PROVIDER_LOOK_PATH,
    lutName: 'Invented Stock',
    lutIntensity: 100,
    rawToneRendering: 'linear',
    agProviderLook: { camera: 'cam2', look: 'look1', before: { rawToneRendering: 'default' } },
  };

  beforeEach(() => offered());

  it('sends the rest now and queues each photo’s own file, keeping the chosen camera', async () => {
    const rest = takeFromPaste([OTHER], pasted);
    expect(rest).toEqual({ exposure: 1 });
    await settled();
    expect(calls).toContainEqual({
      name: 'provider_look_make',
      args: { paths: [OTHER], camera: 'cam2', look: 'look1', options: {}, remake: false, front: false },
    });
  });

  it('of a look the provider does not list leaves it out', async () => {
    const unknown = { ...pasted, agProviderLook: { camera: 'cam2', look: 'gone', before: {} } };
    expect(takeFromPaste([OTHER], unknown)).toEqual({ exposure: 1 });
    await settled();
    expect(namesCalled()).not.toContain('provider_look_make');
  });
});

describe('opening a photo with a look whose file is missing', () => {
  const edit = { lutPath: PROVIDER_LOOK_PATH, agProviderLook: { camera: 'cam1', look: 'look1', before: {} } };

  beforeEach(() => offered());

  it('asks for the file once', async () => {
    await restoreLook(OPEN, edit);
    await restoreLook(OPEN, edit);
    expect(calls.filter((c) => c.name === 'provider_look_make')).toHaveLength(1);
  });

  it('asks for nothing when the file is there', async () => {
    fileThere = true;
    await restoreLook(OPEN, edit);
    expect(namesCalled()).not.toContain('provider_look_make');
  });

  it('asks for nothing when the camera or look is not listed', async () => {
    await restoreLook(OPEN, { ...edit, agProviderLook: { camera: 'gone', look: 'look1', before: {} } });
    await restoreLook(OPEN, { ...edit, agProviderLook: { camera: 'cam1', look: 'gone', before: {} } });
    expect(namesCalled()).not.toContain('provider_look_make');
  });

  it('leaves the edit alone when the file arrives', async () => {
    await restoreLook(OPEN, edit);
    openPhoto(edit);
    const before = useEditorStore.getState().adjustments;
    hear?.({ payload: progress(OPEN, 'done') });
    await settled();
    expect(useEditorStore.getState().adjustments).toEqual(before);
    expect(useEditorStore.getState().adjustments).not.toBe(before);
  });
});

describe('Preview all', () => {
  const state = () => useProviderLooks.getState();
  const previewed = (outcome: LookProgress['outcome'], look = 'look1', path = OPEN): LookProgress => ({
    ...progress(path, outcome, look),
    jobId: PREVIEW_JOB,
    preview: true,
  });
  const starts = () => calls.filter((c) => c.name === 'provider_look_preview');
  const cancels = () => calls.filter((c) => c.name === 'provider_look_cancel');
  const loads = () => calls.filter((c) => c.name === 'provider_looks');
  const hearAll = async (...heard: LookProgress[]) => {
    heard.forEach((payload) => hear?.({ payload }));
    await settled();
  };

  beforeEach(() => offered());

  it('asks once, for this photo with its camera and options', async () => {
    await state().previewAll(OPEN, 'cam2', { optionA: true });
    expect(starts()).toEqual([
      { name: 'provider_look_preview', args: { path: OPEN, camera: 'cam2', options: { optionA: true } } },
    ]);
  });

  it('starts nothing on opening, browsing, a restored edit or a click', async () => {
    const edit = { lutPath: PROVIDER_LOOK_PATH, agProviderLook: { camera: 'cam1', look: 'look1', before: {} } };
    openPhoto(edit);
    await restoreLook(OPEN, edit);
    await state().load(OTHER);
    await state().pick(OPEN, LOOK, 'cam1', {});
    await state().chooseCamera(OPEN, 'cam2', {});
    await settled();
    expect(starts()).toEqual([]);
  });

  it('counts every look, from none', async () => {
    await state().previewAll(OPEN, 'cam1', {});
    expect(state().previewing).toMatchObject({ photo: OPEN, ended: 0, total: 2 });
    await hearAll(previewed('done'));
    expect(state().previewing).toMatchObject({ ended: 1, total: 2 });
  });

  it('ends once every look has answered, one of them failing', async () => {
    await state().previewAll(OPEN, 'cam1', {});
    await hearAll(previewed('failed'), previewed('done', 'look2'));
    expect(state().previewing).toBeNull();
    expect(state().note).toBeNull();
  });

  it('reads the files again as each arrives', async () => {
    await state().previewAll(OPEN, 'cam1', {});
    const before = loads().length;
    await hearAll(previewed('done'));
    expect(loads().length).toBe(before + 1);
  });

  it('leaves the photo as it is', async () => {
    await state().previewAll(OPEN, 'cam1', {});
    const before = useEditorStore.getState().adjustments;
    await hearAll(previewed('done'), previewed('done', 'look2'));
    expect(useEditorStore.getState().adjustments).toBe(before);
  });

  it('runs once at a time', async () => {
    await state().previewAll(OPEN, 'cam1', {});
    await state().previewAll(OPEN, 'cam1', {});
    expect(starts()).toHaveLength(1);
  });

  it('does nothing with no provider or with the switch off', async () => {
    await offered(null);
    await state().previewAll(OPEN, 'cam1', {});
    await offered({ ...OFFER, on: false });
    await state().previewAll(OPEN, 'cam1', {});
    expect(starts()).toEqual([]);
    expect(state().previewing).toBeNull();
  });

  it('stops when another photo opens', async () => {
    await state().previewAll(OPEN, 'cam1', {});
    useEditorStore.setState({ selectedImage: { path: OTHER } as never });
    await settled();
    expect(cancels()).toEqual([{ name: 'provider_look_cancel', args: { jobId: PREVIEW_JOB } }]);
    expect(state().previewing).toBeNull();
  });

  it('stops when the photo closes', async () => {
    await state().previewAll(OPEN, 'cam1', {});
    useEditorStore.setState({ selectedImage: null });
    await settled();
    expect(cancels()).toHaveLength(1);
  });

  it('carries on through other changes to the editor', async () => {
    await state().previewAll(OPEN, 'cam1', {});
    useEditorStore.setState({ adjustments: { exposure: 1 } as never });
    await settled();
    expect(cancels()).toEqual([]);
    expect(state().previewing).not.toBeNull();
  });

  it('has nothing to stop once it has ended', async () => {
    await state().previewAll(OPEN, 'cam1', {});
    await hearAll(previewed('done'), previewed('done', 'look2'));
    useEditorStore.setState({ selectedImage: { path: OTHER } as never });
    await settled();
    expect(cancels()).toEqual([]);
  });

  it('is called off when stopped before the queue answers', async () => {
    const asking = state().previewAll(OPEN, 'cam1', {});
    useEditorStore.setState({ selectedImage: { path: OTHER } as never });
    await asking;
    await settled();
    expect(cancels()).toEqual([{ name: 'provider_look_cancel', args: { jobId: PREVIEW_JOB } }]);
  });

  it('stops on another camera, and does not start again', async () => {
    await state().previewAll(OPEN, 'cam1', {});
    await state().chooseCamera(OPEN, 'cam2', {});
    expect(cancels()).toEqual([{ name: 'provider_look_cancel', args: { jobId: PREVIEW_JOB } }]);
    expect(state().previewing).toBeNull();
    expect(starts()).toHaveLength(1);
  });

  it('stops on another option, and does not start again', async () => {
    await offered(WITH_OPTION);
    await state().previewAll(OPEN, 'cam1', {});
    await state().chooseOption(OPEN, OPTION, true, {});
    expect(cancels()).toEqual([{ name: 'provider_look_cancel', args: { jobId: PREVIEW_JOB } }]);
    expect(starts()).toHaveLength(1);
  });

  it('stops with the switch', async () => {
    await state().previewAll(OPEN, 'cam1', {});
    await state().setOn(false);
    expect(cancels()).toEqual([{ name: 'provider_look_cancel', args: { jobId: PREVIEW_JOB } }]);
    expect(state().previewing).toBeNull();
  });

  it('carries on while a look is picked, and the look goes on as usual', async () => {
    await state().previewAll(OPEN, 'cam1', {});
    await state().pick(OPEN, LOOK, 'cam1', {});
    await hearAll(progress(OPEN, 'done'));
    expect(useEditorStore.getState().adjustments.lutPath).toBe(PROVIDER_LOOK_PATH);
    expect(cancels()).toEqual([]);
    expect(state().previewing).toMatchObject({ ended: 0, total: 2 });
  });

  it('never puts a look on from its own results, with a click waiting for the same look', async () => {
    await state().previewAll(OPEN, 'cam1', {});
    await state().pick(OPEN, OFFER.looks[1], 'cam1', {});
    await hearAll(previewed('done', 'look2'));
    expect(state().waiting[OPEN]?.look).toBe('look2');
    expect(useEditorStore.getState().adjustments.lutPath).toBeUndefined();
  });

  it('does not count a result from a job it stopped', async () => {
    await state().previewAll(OPEN, 'cam1', {});
    await hearAll({ ...previewed('done'), jobId: 3 }, previewed('cancelled'));
    expect(state().previewing).toMatchObject({ ended: 0 });
  });
});

describe('Delete all previews', () => {
  const state = () => useProviderLooks.getState();
  const deletes = () => calls.filter((c) => c.name === 'provider_look_delete');
  const loads = () => calls.filter((c) => c.name === 'provider_looks');
  const ON_PHOTO = { camera: 'cam1', look: 'look1', before: {} };
  const edit = { lutPath: PROVIDER_LOOK_PATH, agProviderLook: ON_PHOTO };

  beforeEach(async () => {
    vi.mocked(toast.info).mockClear();
    vi.mocked(toast.error).mockClear();
    answerDeleted = { deleted: 3, kept: 1, failed: 0 };
    await offered();
  });

  it('asks once, for this photo', async () => {
    await state().deleteLooks(OPEN);
    expect(deletes()).toEqual([{ name: 'provider_look_delete', args: { path: OPEN, onPhoto: null } }]);
  });

  it('names the look on the open edit, whose save may be on its way', async () => {
    openPhoto(edit);
    await state().deleteLooks(OPEN);
    expect(deletes()[0].args).toEqual({ path: OPEN, onPhoto: ON_PHOTO });
  });

  it('does nothing with no provider or with the switch off', async () => {
    await offered(null);
    await state().deleteLooks(OPEN);
    await offered({ ...OFFER, on: false });
    await state().deleteLooks(OPEN);
    expect(deletes()).toEqual([]);
    expect(toast.info).not.toHaveBeenCalled();
  });

  it('runs once at a time', async () => {
    const first = state().deleteLooks(OPEN);
    await state().deleteLooks(OPEN);
    await first;
    expect(deletes()).toHaveLength(1);
    expect(state().deleting).toBe(false);
  });

  it('stops a running Preview all first', async () => {
    await state().previewAll(OPEN, 'cam1', {});
    await state().deleteLooks(OPEN);
    expect(state().previewing).toBeNull();
    const names = namesCalled();
    expect(names.indexOf('provider_look_cancel')).toBeGreaterThan(-1);
    expect(names.indexOf('provider_look_cancel')).toBeLessThan(names.indexOf('provider_look_delete'));
  });

  it('reads the files again straight away, with the photo’s options', async () => {
    await state().chooseOption(OPEN, OPTION, true, {});
    const before = loads().length;
    await state().deleteLooks(OPEN);
    expect(loads().slice(before)).toEqual([
      { name: 'provider_looks', args: { path: OPEN, options: { optionA: true } } },
    ]);
  });

  it('says how many went and how many an edit keeps', async () => {
    await state().deleteLooks(OPEN);
    expect(toast.info).toHaveBeenCalledWith('looksProviderDeleted');
    expect(toast.error).not.toHaveBeenCalled();
  });

  it('says so when a file would not go', async () => {
    answerDeleted = { deleted: 2, kept: 1, failed: 1 };
    await state().deleteLooks(OPEN);
    expect(toast.error).toHaveBeenCalledWith('looksProviderDeleteFailed');
    expect(toast.info).not.toHaveBeenCalled();
  });

  it('survives a refusal, and the files are read again', async () => {
    answerDeleted = null;
    const before = loads().length;
    await state().deleteLooks(OPEN);
    expect(state().deleting).toBe(false);
    expect(toast.info).not.toHaveBeenCalled();
    expect(loads().length).toBe(before + 1);
  });

  it('starts nothing again: the look on the photo keeps its file', async () => {
    fileThere = true;
    openPhoto(edit);
    await restoreLook(OPEN, edit);
    await state().deleteLooks(OPEN);
    await restoreLook(OPEN, edit);
    await settled();
    expect(namesCalled()).not.toContain('provider_look_make');
    expect(namesCalled()).not.toContain('provider_look_preview');
  });

  it('starts nothing again for a photo with no look on it', async () => {
    await state().deleteLooks(OPEN);
    await restoreLook(OPEN, {});
    await settled();
    expect(namesCalled()).not.toContain('provider_look_make');
    expect(namesCalled()).not.toContain('provider_look_preview');
  });

  it('leaves the photo as it is', async () => {
    openPhoto(edit);
    const before = useEditorStore.getState().adjustments;
    await state().deleteLooks(OPEN);
    expect(useEditorStore.getState().adjustments).toBe(before);
  });
});

describe('the previews a photo shows', () => {
  const PREVIEW = { camera: 'cam1', look: 'look1', path: 'C:/cache/.a.cam1.look1.lut.png' };

  beforeEach(async () => {
    answerOffer = { ...OFFER, photo: { camera: 'cam1', files: [], previews: [PREVIEW], beside: 0 } };
    await useProviderLooks.getState().load(OPEN, { optionA: true });
  });

  it('are those of the offer read for this photo and these options', () => {
    expect(previewsFor(useProviderLooks.getState(), OPEN, { optionA: true })).toEqual([PREVIEW]);
  });

  it('are none while the offer is still another photo’s', () => {
    expect(previewsFor(useProviderLooks.getState(), OTHER, { optionA: true })).toEqual([]);
  });

  it('are none while the offer is still for other options', () => {
    expect(previewsFor(useProviderLooks.getState(), OPEN, {})).toEqual([]);
  });

  it('are none with no provider', async () => {
    answerOffer = null;
    await useProviderLooks.getState().load(OPEN, { optionA: true });
    expect(previewsFor(useProviderLooks.getState(), OPEN, { optionA: true })).toEqual([]);
  });
});

describe('lookFor', () => {
  it('reads the look on an edit only while the marker is on', () => {
    const look = { camera: 'cam1', look: 'look1', before: {} };
    expect(lookFor({ lutPath: PROVIDER_LOOK_PATH, agProviderLook: look })).toEqual(look);
    expect(lookFor({ lutPath: 'film.cube', agProviderLook: look })).toBeNull();
  });
});
