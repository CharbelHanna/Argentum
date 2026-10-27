// The demo folder's photos, drawn in the page: a sky, a sun, two ridges and a
// field, in a different light for each. Never a real photo, never a file of
// the repository: nothing here is a picture of anyone or anywhere.
//
// Each photo is drawn once, when the stub loads, and kept as a JPEG data URL
// (thumbnails, previews) and as its bytes (what apply_adjustments answers).

export const DEMO_DRIVE = 'D:\\';
export const DEMO_PARENT = 'D:\\Photos';
export const DEMO_FOLDER = 'D:\\Photos\\Demo';

/** name, size, and the light: sky top, sky bottom, sun, far ridge, near ridge, field. */
const SCENES = [
  ['Demo-01.dng', 1500, 1000, ['#1d3b6e', '#f2a65a', '#ffe08a', '#4a3f6b', '#2c2a45', '#3d5a2a']],
  ['Demo-02.jpg', 1500, 1000, ['#5fa8e8', '#d7ecfb', '#fff6c8', '#6d8fa8', '#3f6a54', '#7fa34a']],
  ['Demo-03.jpg', 1000, 1500, ['#0b1026', '#3a3f7a', '#e8e8f0', '#232a4a', '#151a30', '#1f2b24']],
  ['Demo-04.jpg', 1500, 1000, ['#f5c26b', '#f7e3b5', '#ffffff', '#b98a5a', '#8a5a3a', '#c9a35a']],
  ['Demo-05.jpg', 1500, 1000, ['#8aa0b0', '#c9d3da', '#eef2f5', '#7d8c96', '#5a6870', '#6f7f5f']],
  ['Demo-06.jpg', 1500, 1000, ['#7a2e5c', '#f08a6b', '#ffd2a0', '#5a2a4a', '#3a1a30', '#5a3a2a']],
];

/** What the stub reports as each photo's camera, the same for every run. */
const EXIF = {
  Make: 'Demo',
  Model: 'Camera One',
  LensModel: 'Demo 35mm F2',
  FNumber: '2.8',
  ExposureTime: '1/250',
  PhotographicSensitivity: '200',
  FocalLength: '35',
  DateTimeOriginal: '2026:01:02 10:00:00',
};

function draw(width, height, [skyTop, skyBottom, sun, far, near, field]) {
  const canvas = document.createElement('canvas');
  canvas.width = width;
  canvas.height = height;
  const g = canvas.getContext('2d');
  const sky = g.createLinearGradient(0, 0, 0, height * 0.7);
  sky.addColorStop(0, skyTop);
  sky.addColorStop(1, skyBottom);
  g.fillStyle = sky;
  g.fillRect(0, 0, width, height);
  g.fillStyle = sun;
  g.beginPath();
  g.arc(width * 0.68, height * 0.32, Math.min(width, height) * 0.08, 0, Math.PI * 2);
  g.fill();
  const ridge = (color, base, peaks) => {
    g.fillStyle = color;
    g.beginPath();
    g.moveTo(0, height);
    peaks.forEach(([x, y]) => g.lineTo(width * x, height * (base - y)));
    g.lineTo(width, height);
    g.closePath();
    g.fill();
  };
  ridge(far, 0.62, [
    [0, 0.02],
    [0.18, 0.14],
    [0.35, 0.05],
    [0.55, 0.2],
    [0.78, 0.06],
    [1, 0.12],
  ]);
  ridge(near, 0.72, [
    [0, 0.08],
    [0.25, 0.02],
    [0.45, 0.12],
    [0.7, 0.03],
    [1, 0.1],
  ]);
  const ground = g.createLinearGradient(0, height * 0.72, 0, height);
  ground.addColorStop(0, field);
  ground.addColorStop(1, '#101010');
  g.fillStyle = ground;
  g.fillRect(0, height * 0.72, width, height * 0.28);
  return canvas.toDataURL('image/jpeg', 0.85);
}

function bytesOf(dataUrl) {
  const binary = atob(dataUrl.slice(dataUrl.indexOf(',') + 1));
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) bytes[i] = binary.charCodeAt(i);
  return bytes.buffer;
}

export const PHOTOS = SCENES.map(([name, width, height, light], index) => {
  const dataUrl = draw(width, height, light);
  return {
    name,
    path: `${DEMO_FOLDER}\\${name}`,
    width,
    height,
    isRaw: name.endsWith('.dng'),
    // A minute apart, so sorting by date is the same as by name.
    modified: 1767348000 + index * 60,
    exif: { ...EXIF },
    dataUrl,
    bytes: bytesOf(dataUrl),
  };
});

export const photoAt = (path) => PHOTOS.find((photo) => photo.path === path);
