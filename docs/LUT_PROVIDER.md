# LUT provider protocol

A LUT provider is a local program that makes a LUT for one photo. It is optional, and off unless you
add it by hand. Argentum never ships one and never writes the setting.

## Turning it on

Add one line to `argentum-processing.json`, in the `camera-profiles` folder of Argentum's data folder:

```json
{ "lutProvider": "C:\\Tools\\my-looks\\my-looks.exe" }
```

The value is the absolute path of one local program: on Windows an `.exe`, elsewhere a file with an
executable bit. Network paths and paths with `..` are refused. Absent or empty means off: no process
runs and nothing changes in the app. Changing the field needs a restart.

## Calling it

Argentum runs `<program> list` or `<program> lut`, with no shell, in the program's folder. That one
word is the only argument. The request is one JSON object on stdin and the answer one JSON object on
stdout, both UTF-8. Keep stdout clean: anything else there breaks the protocol, so send logging to
stderr, which Argentum writes to its log. Each stream is capped at 64 KB.

Exit 0 with `"ok": false` is a reported failure. A non-zero exit, a timeout or an answer that isn't
JSON counts as a crash. Two crashes in a row turn the provider off until the app closes.

### `list`

Called once, the first time the Looks tab opens. It must answer within 10 seconds.

```json
{ "protocol": 1 }
```

```json
{
  "protocol": 1,
  "generator": "my-looks 1.0.0",
  "cameras": [{ "id": "cam-a", "name": "Camera A", "make": "Acme", "model": "Alpha" }],
  "looks": [{ "id": "stock-1", "name": "Stock One", "group": "Colour" }]
}
```

- Ids match `^[a-z0-9][a-z0-9-]{0,15}$` and are unique within their list.
- Names are 1 to 80 characters. A camera needs a `make` and a `model`; a look's `group` is optional.
- A bad entry is dropped and the rest stay. With no camera or no look left, the provider is off.
- `generator` only goes to the log.
- A photo is matched to a camera by `make` and `model`. A photo that matches none shows no provider
  looks.

### `lut`

```json
{
  "protocol": 1,
  "camera": "cam-a",
  "look": "stock-1",
  "photo": { "path": "D:\\Photos\\IMG_0001.RAW", "make": "Acme", "model": "Alpha", "iso": 400 },
  "asShotWb": [1.8, 1.0, 1.0, 1.6],
  "rendering": { "mode": "linear", "offsetEv": 0.25 },
  "output": "C:\\...\\lut_provider\\1.png",
  "grid": 64
}
```

| Field                       | Meaning                                                                                                                        |
| --------------------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| `camera`                    | The chosen camera id from `list`, which may not be the photo's own                                                             |
| `photo.make`, `photo.model` | Always the real camera                                                                                                         |
| `photo.path`                | For the program's log only. The program must not open the photo                                                                |
| `asShotWb`                  | The as-shot white balance as R, G1, G2, B, with G1 = 1                                                                         |
| `rendering`                 | The LUT goes after Linear rendering, so it sees sRGB-encoded values. `offsetEv` is the exposure offset Linear gives this photo |
| `output`                    | The one path the program may write: a 512 × 512, 8-bit RGB PNG in cube order, red fastest, at most 2 MB                        |
| `grid`                      | 64                                                                                                                             |

It has 60 seconds. The answer is one of:

```json
{ "ok": true, "seconds": 1.2 }
```

```json
{ "ok": false, "error": "data_missing", "message": "Shown in the log" }
```

Error codes: `unsupported_camera`, `unknown_look`, `bad_request`, `data_missing`, `internal`. An
unknown code counts as `internal`.

## What Argentum does with it

- It checks the PNG, then writes it beside the photo as `<photo file name>.<camera>.<look>.lut.png`.
  The program never touches the photo's folder, and no path is taken from an answer.
- Requests with the same camera, look, make, model, white balance and offset share one run.
- Two runs happen at a time. A run nobody is waiting for any more is killed.
- A file already beside the photo is used with no call, so a finished photo never changes by itself.
- Everything a provider returns is treated as untrusted, and shown as plain text.

A provider runs with all of your rights: it isn't a sandbox. Only add a program you trust.
