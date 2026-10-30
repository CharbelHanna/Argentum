//! A local program that makes a LUT for one photo. Ours.
//!
//! An optional setting, `lutProvider` in `argentum-processing.json`, names one
//! local program. Argentum never ships or writes that field: only a hand edit
//! adds it, and absent or empty means off, with nothing shown and nothing run.
//! One run per request, one word on the command line, one JSON object on stdin
//! and one on stdout. A run that never starts costs nothing.
//!
//! Only this module runs the program, and nothing here can write the field, so
//! no command, edit or answer can choose what runs. An edit can only name a
//! camera and a look, and those count only if the program itself lists them.
//! Whatever comes back is treated as untrusted: its size is capped, its ids must
//! fit a file name, its PNG is checked, and no path is taken from it. The
//! program writes one scratch file named here, and only Argentum writes beside a
//! photo, whole or not at all (`write_atomic`).
//!
//! The [`Queue`] holds the work: photos asking for the same thing share a run, a
//! photo waits for one look at a time, two run at once, and a run nobody waits
//! for any more is killed. Rust only makes files. Putting a look on a photo is
//! the interface's, as for any look, so Undo stays in one place
//! (src/argentum/looks/).

use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{Cursor, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, mpsc};
use std::time::{Duration, Instant};

use image::{ImageFormat, ImageReader};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager};

use crate::AppState;
use crate::file_management::{parse_virtual_path, read_file_mapped};
use crate::lut_processing::{self, Lut};
use crate::mods::ag_settings::Setting;
use crate::mods::atomic_write::write_atomic;
use crate::mods::profiles::{self, same_model};
use crate::mods::{dng_standard, file_ops, provider_look};

/// The event that reports each photo's result.
pub const EVENT: &str = "provider-look-progress";

/// The program. Read here and nowhere else; never saved.
const PROGRAM: Setting<String> = Setting::new("lutProvider");

/// The Provider looks switch, saved only when the user flips it.
const SWITCH: Setting<bool> = Setting::new("providerLooksOn");

const PROTOCOL: u32 = 1;
/// Steps of the LUT; a PNG LUT must be square, so 65 cannot be stored as one.
const GRID: u32 = 64;
/// The side of a 64-step LUT's PNG, in pixels.
const SIDE_PX: u32 = 512;
const MAX_FILE_BYTES: usize = 2 * 1024 * 1024;
/// What is read of stdout and of stderr (the log's, only).
const MAX_OUTPUT_BYTES: usize = 64 * 1024;
const MAX_NAME_CHARS: usize = 80;
const RUNS_AT_ONCE: usize = 2;
/// In a row, before the provider is off until the app closes.
const CRASHES_TO_OFF: u32 = 2;
/// How long a closing app waits for its runs to go.
const STOP_WAIT: Duration = Duration::from_secs(2);
/// How often an export waiting for a look looks at its cancellation token.
const EXPORT_POLL: Duration = Duration::from_millis(100);

/// How long the program gets.
#[derive(Clone, Copy)]
pub struct Limits {
    list: Duration,
    lut: Duration,
}

const LIMITS: Limits = Limits {
    list: Duration::from_secs(10),
    // About ten times the slowest run expected.
    lut: Duration::from_secs(60),
};

/// A camera the program offers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct OfferedCamera {
    pub id: String,
    pub name: String,
    pub make: String,
    pub model: String,
}

/// A look the program offers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct OfferedLook {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub group: Option<String>,
}

/// What `list` answered, after validation.
#[derive(Debug, Clone, PartialEq)]
pub struct Catalog {
    pub cameras: Vec<OfferedCamera>,
    pub looks: Vec<OfferedLook>,
}

// ------------------------------------------------------------------ the setting

/// The program the setting names; None when it is absent or empty.
fn configured(library: &Path) -> Option<String> {
    PROGRAM
        .load(library)
        .filter(|program| !program.trim().is_empty())
}

/// Whether provider looks are the usable list. Starts on.
fn is_on(library: &Path) -> bool {
    SWITCH.load(library).unwrap_or(true)
}

/// Remembers the switch, only for someone with a provider.
fn set_on(library: &Path, on: bool) -> Result<(), String> {
    if configured(library).is_none() {
        return Err("no provider is set".to_string());
    }
    SWITCH.save(library, &on)
}

// ------------------------------------------------------------------ validation

/// The program's path, if it is one that may run: absolute, local, no `..`, a
/// file, and runnable as it stands. A `.cmd` would go through a shell.
fn check_path(program: &str) -> Result<PathBuf, String> {
    let path = Path::new(program);
    if program.starts_with(r"\\") || program.starts_with("//") {
        return Err("a network path".to_string());
    }
    if !path.is_absolute() {
        return Err("not a full path".to_string());
    }
    if path.components().any(|c| c == Component::ParentDir) {
        return Err("a path with ..".to_string());
    }
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if !meta.is_file() {
        return Err("not a file".to_string());
    }
    if !is_runnable(path, &meta) {
        return Err("not a program".to_string());
    }
    Ok(path.to_path_buf())
}

#[cfg(windows)]
fn is_runnable(path: &Path, _meta: &std::fs::Metadata) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
}

#[cfg(unix)]
fn is_runnable(_path: &Path, meta: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    meta.permissions().mode() & 0o111 != 0
}

/// The entries that read as a `T` and pass, the first of each id.
fn kept<T: DeserializeOwned>(
    entries: Vec<Value>,
    id_of: impl Fn(&T) -> &str,
    good: impl Fn(&T) -> bool,
) -> Vec<T> {
    let mut seen = HashSet::new();
    entries
        .into_iter()
        .filter_map(|entry| serde_json::from_value::<T>(entry).ok())
        .filter(|entry| {
            file_ops::is_look_name_part(id_of(entry))
                && good(entry)
                && seen.insert(id_of(entry).to_string())
        })
        .collect()
}

fn good_name(name: &str) -> bool {
    !name.trim().is_empty() && name.chars().count() <= MAX_NAME_CHARS
}

/// What `list` answered. A bad entry is dropped and the rest stay; with no
/// camera or no look left there is nothing to offer.
fn read_list(answer: &[u8]) -> Result<Catalog, String> {
    #[derive(Deserialize)]
    struct List {
        protocol: u32,
        generator: Option<String>,
        #[serde(default)]
        cameras: Vec<Value>,
        #[serde(default)]
        looks: Vec<Value>,
    }
    let list: List = serde_json::from_slice(answer).map_err(|e| e.to_string())?;
    if list.protocol != PROTOCOL {
        return Err(format!("protocol {}", list.protocol));
    }
    let cameras = kept(
        list.cameras,
        |c: &OfferedCamera| &c.id,
        |c| good_name(&c.name) && !c.make.trim().is_empty() && !c.model.trim().is_empty(),
    );
    let looks = kept(list.looks, |l: &OfferedLook| &l.id, |l| good_name(&l.name));
    if cameras.is_empty() || looks.is_empty() {
        return Err("no camera or no look on the list".to_string());
    }
    if let Some(generator) = list.generator {
        log::info!("LUT provider: {generator}");
    }
    Ok(Catalog { cameras, looks })
}

/// Checks the program and asks what it offers.
fn validate(program: &str, limits: &Limits) -> Result<Catalog, String> {
    let exe = check_path(program)?;
    let request = json!({ "protocol": PROTOCOL }).to_string();
    let never = AtomicBool::new(false);
    let answer = run(&exe, "list", request.as_bytes(), limits.list, &never)
        .map_err(|failure| format!("{failure:?}"))?;
    read_list(&answer)
}

/// The program in the setting and what it offers. For everyone without the
/// field this ends at the first line, with no process run.
fn offered(library: &Path, limits: &Limits) -> Option<(String, Catalog)> {
    let program = configured(library)?;
    match validate(&program, limits) {
        Ok(catalog) => Some((program, catalog)),
        Err(reason) => {
            log::warn!("LUT provider is off: {reason}");
            None
        }
    }
}

// ------------------------------------------------------------------ one run

/// Why a run gave no LUT.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Failure {
    /// It did not start, exited with an error, or answered nonsense.
    Crash,
    Timeout,
    Cancelled,
    /// Its answer was fine and its file was not.
    BadFile,
    /// It said so, with `ok: false`.
    Refused(Refusal),
}

/// The codes `ok: false` carries; one nobody knows counts as `Internal`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Refusal {
    UnsupportedCamera,
    UnknownLook,
    BadRequest,
    DataMissing,
    Internal,
}

/// What became of one photo's request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum LookOutcome {
    /// Its look file is there.
    Done,
    Cancelled,
    /// "Couldn't make this look".
    Failed,
    /// The program does not offer this camera or look for it.
    Unsupported,
    /// Its folder can't be written.
    Folder,
}

impl Failure {
    fn outcome(self) -> LookOutcome {
        match self {
            Self::Cancelled => LookOutcome::Cancelled,
            Self::Refused(Refusal::UnsupportedCamera | Refusal::UnknownLook) => {
                LookOutcome::Unsupported
            }
            _ => LookOutcome::Failed,
        }
    }

    /// Whether the same request would fail the same way this session.
    fn is_final(self) -> bool {
        matches!(
            self,
            Self::Refused(Refusal::UnsupportedCamera | Refusal::UnknownLook | Refusal::DataMissing)
        )
    }
}

/// Reads a pipe up to the cap in the background, then keeps draining it so the
/// program never blocks on a full pipe. The answer is what was read, and
/// whether there was more.
fn capped(pipe: impl Read + Send + 'static) -> mpsc::Receiver<(Vec<u8>, bool)> {
    let (tell, told) = mpsc::channel();
    std::thread::spawn(move || {
        let mut pipe = pipe;
        let mut kept = Vec::new();
        let mut more = false;
        let mut chunk = [0u8; 8192];
        while let Ok(n) = pipe.read(&mut chunk) {
            if n == 0 {
                break;
            }
            let room = (MAX_OUTPUT_BYTES + 1).saturating_sub(kept.len()).min(n);
            kept.extend_from_slice(&chunk[..room]);
            more |= kept.len() > MAX_OUTPUT_BYTES;
        }
        kept.truncate(MAX_OUTPUT_BYTES);
        let _ = tell.send((kept, more));
    });
    told
}

/// The program and all it started, now.
fn kill(child: &mut Child) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let system = std::env::var_os("SystemRoot").map_or_else(PathBuf::new, PathBuf::from);
        let _ = Command::new(system.join("System32").join("taskkill.exe"))
            .args(["/T", "/F", "/PID", &child.id().to_string()])
            .creation_flags(NO_WINDOW)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// `CREATE_NO_WINDOW`.
#[cfg(windows)]
const NO_WINDOW: u32 = 0x0800_0000;

/// One run of the program: no shell, one fixed word, `input` on stdin, in its
/// own folder. What it wrote to stdout, or why not.
fn run(
    exe: &Path,
    word: &str,
    input: &[u8],
    timeout: Duration,
    cancel: &AtomicBool,
) -> Result<Vec<u8>, Failure> {
    let mut command = Command::new(exe);
    command
        .arg(word)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(folder) = exe.parent() {
        command.current_dir(folder);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(NO_WINDOW);
    }
    let mut child = command.spawn().map_err(|e| {
        log::warn!("LUT provider: could not start: {e}");
        Failure::Crash
    })?;

    let (Some(mut stdin), Some(stdout), Some(stderr)) =
        (child.stdin.take(), child.stdout.take(), child.stderr.take())
    else {
        kill(&mut child);
        return Err(Failure::Crash);
    };
    let input = input.to_vec();
    std::thread::spawn(move || {
        // A program that does not read its stdin is its own affair.
        let _ = stdin.write_all(&input);
    });
    let (stdout, stderr) = (capped(stdout), capped(stderr));

    let started = Instant::now();
    let status = loop {
        if cancel.load(Ordering::SeqCst) {
            kill(&mut child);
            return Err(Failure::Cancelled);
        }
        if started.elapsed() >= timeout {
            log::warn!("LUT provider: no answer to {word} in {timeout:?}");
            kill(&mut child);
            return Err(Failure::Timeout);
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => std::thread::sleep(Duration::from_millis(5)),
            Err(e) => {
                log::warn!("LUT provider: {e}");
                kill(&mut child);
                return Err(Failure::Crash);
            }
        }
    };

    // Something it started may still hold the pipes, so this does not wait long.
    let pipe_wait = Duration::from_secs(1);
    if let Ok((text, _)) = stderr.recv_timeout(pipe_wait)
        && !text.is_empty()
    {
        log::info!("LUT provider: {}", String::from_utf8_lossy(&text));
    }
    if !status.success() {
        log::warn!("LUT provider: {word} ended with {status}");
        return Err(Failure::Crash);
    }
    match stdout.recv_timeout(pipe_wait) {
        Ok((text, false)) => Ok(text),
        _ => {
            log::warn!("LUT provider: {word} answered too much or too late");
            Err(Failure::Crash)
        }
    }
}

/// The checks a LUT passes before it goes anywhere near a photo: small, 512 x
/// 512 (read from its header, before a byte is decoded), a 64-step LUT.
fn checked_file(bytes: &[u8]) -> Result<Lut, Failure> {
    if bytes.len() > MAX_FILE_BYTES {
        return Err(Failure::BadFile);
    }
    let reader = || ImageReader::with_format(Cursor::new(bytes), ImageFormat::Png);
    let side = reader().into_dimensions().map_err(|_| Failure::BadFile)?;
    if side != (SIDE_PX, SIDE_PX) {
        return Err(Failure::BadFile);
    }
    let image = reader().decode().map_err(|_| Failure::BadFile)?;
    let lut = lut_processing::parse_hald(image).map_err(|_| Failure::BadFile)?;
    (lut.size == GRID).then_some(lut).ok_or(Failure::BadFile)
}

/// One photo, as the program is told of it.
#[derive(Debug, Clone)]
pub struct Subject {
    /// As the editor names it, a virtual copy's own path for a virtual copy.
    photo: String,
    make: String,
    model: String,
    iso: Option<f32>,
    /// As-shot white balance as the program is sent it: R, G, G, B, at the
    /// decoder's scale, where green is 1 in most formats but not all.
    wb: [f32; 4],
    /// The exposure offset Linear gives this photo.
    offset_ev: f32,
}

/// The file the run left, or why not.
fn ask_for_lut(
    exe: &Path,
    limits: &Limits,
    subject: &Subject,
    camera: &str,
    look: &str,
    scratch: &Path,
    cancel: &AtomicBool,
) -> Result<(Vec<u8>, Lut), Failure> {
    let request = json!({
        "protocol": PROTOCOL,
        "camera": camera,
        "look": look,
        "photo": {
            "path": parse_virtual_path(&subject.photo).0,
            "make": subject.make,
            "model": subject.model,
            "iso": subject.iso,
        },
        "asShotWb": subject.wb,
        "rendering": { "mode": "linear", "offsetEv": subject.offset_ev },
        "output": scratch,
        "grid": GRID,
    });
    let answer = run(
        exe,
        "lut",
        request.to_string().as_bytes(),
        limits.lut,
        cancel,
    )?;

    #[derive(Deserialize)]
    struct Answer {
        ok: bool,
        error: Option<String>,
        message: Option<String>,
    }
    let answer: Answer = serde_json::from_slice(&answer).map_err(|e| {
        log::warn!("LUT provider: its answer is not JSON: {e}");
        Failure::Crash
    })?;
    if !answer.ok {
        log::info!(
            "LUT provider: {camera} {look}: {:?} {:?}",
            answer.error,
            answer.message
        );
        return Err(Failure::Refused(match answer.error.as_deref() {
            Some("unsupported_camera") => Refusal::UnsupportedCamera,
            Some("unknown_look") => Refusal::UnknownLook,
            Some("bad_request") => Refusal::BadRequest,
            Some("data_missing") => Refusal::DataMissing,
            _ => Refusal::Internal,
        }));
    }
    let bytes = std::fs::read(scratch).map_err(|_| Failure::BadFile)?;
    let lut = checked_file(&bytes)?;
    Ok((bytes, lut))
}

/// [`ask_for_lut`], whose scratch file never outlives the run.
fn make_lut(
    exe: &Path,
    limits: &Limits,
    subject: &Subject,
    camera: &str,
    look: &str,
    scratch: &Path,
    cancel: &AtomicBool,
) -> Result<(Vec<u8>, Lut), Failure> {
    let made = ask_for_lut(exe, limits, subject, camera, look, scratch, cancel);
    let _ = std::fs::remove_file(scratch);
    made
}

// ------------------------------------------------------------------ the queue

/// One photo's result, as the interface hears it.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct LookProgress {
    pub job_id: u64,
    pub path: String,
    pub camera: String,
    pub look: String,
    pub outcome: LookOutcome,
}

/// Where one request's result goes.
pub type Report = Arc<dyn Fn(LookProgress) + Send + Sync>;

/// One photo asking for a camera's look.
pub struct Request {
    subject: Subject,
    camera: String,
    look: String,
    /// Overwrite a file already there.
    remake: bool,
    /// The open photo: ahead of what is waiting.
    front: bool,
    job: u64,
    report: Report,
}

/// What a LUT depends on: requests with the same key share a run.
#[derive(Clone, PartialEq, Eq)]
struct Key {
    camera: String,
    look: String,
    make: String,
    model: String,
    wb: [u32; 4],
    offset: u32,
}

impl Key {
    fn of(subject: &Subject, camera: &str, look: &str) -> Self {
        Self {
            camera: camera.to_string(),
            look: look.to_string(),
            make: subject.make.clone(),
            model: subject.model.clone(),
            wb: subject.wb.map(f32::to_bits),
            offset: subject.offset_ev.to_bits(),
        }
    }
}

/// A photo waiting for an item.
struct Waiter {
    photo: String,
    job: u64,
    file: PathBuf,
    report: Report,
}

impl Waiter {
    fn tell(&self, camera: &str, look: &str, outcome: LookOutcome) {
        (self.report)(LookProgress {
            job_id: self.job,
            path: self.photo.clone(),
            camera: camera.to_string(),
            look: look.to_string(),
            outcome,
        });
    }
}

/// One run to make, and who is waiting for it.
struct Item {
    key: Key,
    subject: Subject,
    waiters: Vec<Waiter>,
    /// Set once nobody waits: the run is killed.
    cancel: Arc<AtomicBool>,
}

#[derive(Default)]
struct State {
    waiting: VecDeque<Item>,
    running: Vec<Item>,
    crashes: u32,
    off: bool,
    /// What the program said no to: (camera, look), not asked again.
    refused: HashMap<(String, String), LookOutcome>,
    scratch_files: u64,
}

impl State {
    fn items(&mut self) -> impl Iterator<Item = &mut Item> {
        self.waiting.iter_mut().chain(self.running.iter_mut())
    }

    /// Takes the waiters `leaves` picks, and kills or forgets the items that
    /// leaves empty.
    fn take_waiters(
        &mut self,
        leaves: impl Fn(&Item, &Waiter) -> bool,
    ) -> Vec<(String, String, Waiter)> {
        let mut gone = Vec::new();
        for item in self.items() {
            let (camera, look) = (item.key.camera.clone(), item.key.look.clone());
            let mut kept = Vec::new();
            for waiter in std::mem::take(&mut item.waiters) {
                if leaves(item, &waiter) {
                    gone.push((camera.clone(), look.clone(), waiter));
                } else {
                    kept.push(waiter);
                }
            }
            item.waiters = kept;
            if item.waiters.is_empty() {
                item.cancel.store(true, Ordering::SeqCst);
            }
        }
        self.waiting.retain(|item| !item.waiters.is_empty());
        gone
    }
}

/// Told when a photo's look file is written: its LUT replaces any in memory.
type Placed = Box<dyn Fn(&str, &Path, Arc<Lut>) + Send + Sync>;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

/// The work for one program.
pub struct Queue {
    exe: PathBuf,
    scratch: PathBuf,
    limits: Limits,
    placed: Placed,
    state: Mutex<State>,
}

impl Queue {
    /// A queue with an empty scratch folder: what a run left behind goes.
    fn new(exe: PathBuf, scratch: PathBuf, limits: Limits, placed: Placed) -> Arc<Self> {
        let _ = std::fs::remove_dir_all(&scratch);
        let _ = std::fs::create_dir_all(&scratch);
        Arc::new(Self {
            exe,
            scratch,
            limits,
            placed,
            state: Mutex::new(State::default()),
        })
    }

    fn state(&self) -> MutexGuard<'_, State> {
        lock(&self.state)
    }

    /// Whether crashes turned the provider off until the app closes.
    fn is_off(&self) -> bool {
        self.state().off
    }

    /// Whether anything is queued or running.
    #[cfg(test)]
    fn is_busy(&self) -> bool {
        let state = self.state();
        !state.waiting.is_empty() || !state.running.is_empty()
    }

    /// Asks for a photo's look file. The answer comes through `report`, at once
    /// if nothing needs a run. A photo waits for one thing: the latest.
    pub fn request(self: &Arc<Self>, request: Request) {
        let Request {
            subject,
            camera,
            look,
            remake,
            front,
            job,
            report,
        } = request;
        let key = Key::of(&subject, &camera, &look);
        let source = parse_virtual_path(&subject.photo).0;
        let Some(file) = file_ops::look_file_for(&source, &camera, &look) else {
            return;
        };
        let waiter = Waiter {
            photo: subject.photo.clone(),
            job,
            file,
            report,
        };

        let replaced = {
            let photo = subject.photo.clone();
            let same = key.clone();
            self.state()
                .take_waiters(|item, w| w.photo == photo && item.key != same)
        };
        for (camera, look, old) in replaced {
            old.tell(&camera, &look, LookOutcome::Cancelled);
        }

        if !remake && std::fs::read(&waiter.file).is_ok_and(|bytes| checked_file(&bytes).is_ok()) {
            waiter.tell(&camera, &look, LookOutcome::Done);
            return;
        }
        if !folder_takes_files(&waiter.file) {
            waiter.tell(&camera, &look, LookOutcome::Folder);
            return;
        }

        let mut state = self.state();
        let refused = state.refused.get(&(camera.clone(), look.clone())).copied();
        match (state.off, refused) {
            (true, _) => {
                drop(state);
                waiter.tell(&camera, &look, LookOutcome::Failed);
                return;
            }
            (false, Some(outcome)) if !remake => {
                drop(state);
                waiter.tell(&camera, &look, outcome);
                return;
            }
            _ => {}
        }

        let joined = state
            .items()
            .find(|item| item.key == key && !item.cancel.load(Ordering::SeqCst));
        if let Some(item) = joined {
            item.waiters.push(waiter);
        } else {
            let item = Item {
                key: key.clone(),
                subject,
                waiters: vec![waiter],
                cancel: Arc::new(AtomicBool::new(false)),
            };
            state.waiting.push_back(item);
        }
        if front
            && let Some(at) = state.waiting.iter().position(|item| item.key == key)
            && let Some(item) = state.waiting.remove(at)
        {
            state.waiting.push_front(item);
        }
        self.start_more(&mut state);
    }

    /// Asks for a photo's look file and waits for the answer, or for `cancel`.
    /// For an export, which has its own thread to wait on.
    fn wait_for(
        self: &Arc<Self>,
        subject: Subject,
        camera: String,
        look: String,
        cancel: &AtomicBool,
    ) -> LookOutcome {
        let job = NEXT_JOB.fetch_add(1, Ordering::SeqCst);
        let (tell, told) = mpsc::channel();
        let tell = Mutex::new(tell);
        self.request(Request {
            subject,
            camera,
            look,
            remake: false,
            front: false,
            job,
            report: Arc::new(move |progress| {
                let _ = lock(&tell).send(progress.outcome);
            }),
        });
        loop {
            match told.recv_timeout(EXPORT_POLL) {
                Ok(outcome) => return outcome,
                Err(mpsc::RecvTimeoutError::Disconnected) => return LookOutcome::Failed,
                Err(mpsc::RecvTimeoutError::Timeout) if cancel.load(Ordering::SeqCst) => {
                    self.cancel(job);
                    return LookOutcome::Cancelled;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
        }
    }

    /// Takes every photo of `job` out of the queue.
    pub fn cancel(&self, job: u64) {
        self.cancel_where(|waiter| waiter.job == job);
    }

    fn cancel_where(&self, leaves: impl Fn(&Waiter) -> bool) {
        let gone = self.state().take_waiters(|_, waiter| leaves(waiter));
        for (camera, look, waiter) in gone {
            waiter.tell(&camera, &look, LookOutcome::Cancelled);
        }
    }

    /// The app is closing: nothing stays behind it, so this returns once the
    /// runs are gone, or after a moment if one will not go.
    fn stop_all(&self) {
        self.cancel_where(|_| true);
        let deadline = Instant::now() + STOP_WAIT;
        while !self.state().running.is_empty() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn start_more(self: &Arc<Self>, state: &mut State) {
        while state.running.len() < RUNS_AT_ONCE {
            let Some(item) = state.waiting.pop_front() else {
                return;
            };
            let (subject, key, cancel) = (
                item.subject.clone(),
                item.key.clone(),
                Arc::clone(&item.cancel),
            );
            state.running.push(item);
            state.scratch_files += 1;
            let scratch = self.scratch.join(format!("{}.png", state.scratch_files));
            let queue = Arc::clone(self);
            std::thread::spawn(move || queue.work(&subject, &key, &scratch, &cancel));
        }
    }

    /// One run, then its results.
    fn work(
        self: Arc<Self>,
        subject: &Subject,
        key: &Key,
        scratch: &Path,
        cancel: &Arc<AtomicBool>,
    ) {
        let made = make_lut(
            &self.exe,
            &self.limits,
            subject,
            &key.camera,
            &key.look,
            scratch,
            cancel,
        );

        let mut state = self.state();
        let at = state
            .running
            .iter()
            .position(|item| Arc::ptr_eq(&item.cancel, cancel));
        let waiters = at.map_or_else(Vec::new, |at| state.running.remove(at).waiters);
        let mut lost = Vec::new();
        match &made {
            Ok(_) => state.crashes = 0,
            Err(Failure::Crash) => {
                state.crashes += 1;
                if state.crashes >= CRASHES_TO_OFF {
                    log::warn!("LUT provider: off until the app closes, it keeps crashing");
                    state.off = true;
                    lost.extend(state.waiting.drain(..));
                }
            }
            Err(failure) if failure.is_final() => {
                let id = (key.camera.clone(), key.look.clone());
                state.refused.insert(id, failure.outcome());
            }
            Err(_) => {}
        }
        self.start_more(&mut state);
        drop(state);

        for item in lost {
            for waiter in &item.waiters {
                waiter.tell(&item.key.camera, &item.key.look, LookOutcome::Failed);
            }
        }
        match made {
            Ok((bytes, lut)) => {
                let lut = Arc::new(lut);
                for waiter in waiters {
                    let outcome = match write_atomic(&waiter.file, &bytes) {
                        Ok(()) => {
                            (self.placed)(&waiter.photo, &waiter.file, Arc::clone(&lut));
                            LookOutcome::Done
                        }
                        Err(e) => {
                            log::warn!("LUT provider: {}: {e}", waiter.file.display());
                            LookOutcome::Folder
                        }
                    };
                    waiter.tell(&key.camera, &key.look, outcome);
                }
            }
            Err(failure) => {
                for waiter in waiters {
                    waiter.tell(&key.camera, &key.look, failure.outcome());
                }
            }
        }
    }
}

/// Whether a file can be created beside `file`: asked before any run.
fn folder_takes_files(file: &Path) -> bool {
    file.parent()
        .is_some_and(|folder| tempfile::NamedTempFile::new_in(folder).is_ok())
}

/// The camera a photo gets: `chosen` while it is on the list, otherwise its own.
/// A photo from no camera on the list gets none, chosen or not.
fn camera_for<'a>(
    catalog: &'a Catalog,
    chosen: Option<&str>,
    make: &str,
    model: &str,
) -> Option<&'a str> {
    let own = catalog
        .cameras
        .iter()
        .find(|c| same_model(&c.make, make) && same_model(&c.model, model))?;
    let chosen = chosen.and_then(|id| catalog.cameras.iter().find(|c| c.id == id));
    Some(&chosen.unwrap_or(own).id)
}

// ------------------------------------------------------------------ the app's

struct Provider {
    catalog: Catalog,
    queue: Arc<Queue>,
}

/// A good provider, kept until the app closes.
static READY: OnceLock<Provider> = OnceLock::new();
/// One validation at a time.
static CHECKING: Mutex<()> = Mutex::new(());
static NEXT_JOB: AtomicU64 = AtomicU64::new(1);

/// The provider, validated the first time this is asked, never at start-up. A
/// bad one is tried again the next time. None for everyone without the field,
/// with no process run.
fn provider(app: &AppHandle) -> Option<&'static Provider> {
    let ready = |provider: &'static Provider| (!provider.queue.is_off()).then_some(provider);
    if let Some(provider) = READY.get() {
        return ready(provider);
    }
    let _one = lock(&CHECKING);
    if let Some(provider) = READY.get() {
        return ready(provider);
    }
    let (program, catalog) = offered(profiles::library()?, &LIMITS)?;
    let scratch = app.path().app_cache_dir().ok()?.join("lut_provider");
    let handle = app.clone();
    let placed = Box::new(move |photo: &str, file: &Path, lut: Arc<Lut>| {
        let state = handle.state::<AppState>();
        lock(&state.lut_cache).insert(file.to_string_lossy().into_owned(), lut);
        refresh_thumbnail(&handle, photo, file);
    });
    Some(READY.get_or_init(|| Provider {
        catalog,
        queue: Queue::new(PathBuf::from(&program), scratch, LIMITS, placed),
    }))
}

/// A photo whose edit names this file has a thumbnail made without it, and
/// its edit text has not changed, so the usual refresh would keep that one.
fn refresh_thumbnail(app: &AppHandle, photo: &str, file: &Path) {
    let sidecar = parse_virtual_path(photo).1;
    let edit = crate::exif_processing::load_sidecar(&sidecar).adjustments;
    if provider_look::look_file(&edit, photo).as_deref() != Some(file) {
        return;
    }
    let state = app.state::<AppState>();
    crate::file_management::add_to_thumbnail_queue(&state, 1, app);
    let settings = crate::app_settings::load_settings(app.clone()).unwrap_or_default();
    let photos = std::iter::once(photo.to_string());
    crate::file_management::regenerate_thumbnails(photos, app, &settings, true);
}

/// The white balance as the program is told of it, from the decoder's R, G, B
/// and fourth colour; None for a photo with none.
fn wb_for_program(c: [f32; 4]) -> Option<[f32; 4]> {
    let wb = if c[3].is_nan() {
        [c[0], c[1], c[1], c[2]]
    } else {
        c
    };
    if wb.iter().any(|c| c.is_nan()) {
        return None;
    }
    Some(wb)
}

/// One photo, read for a request; None for anything but a RAW with a white
/// balance.
fn subject_of(photo: &str) -> Option<Subject> {
    let source = parse_virtual_path(photo).0;
    if !crate::formats::is_raw_file(&source) {
        return None;
    }
    let bytes = read_file_mapped(&source).ok()?;
    let facts = dng_standard::facts(&bytes).ok()?;
    let wb = wb_for_program(crate::as_shot::as_shot(photo).wb_coeffs)?;
    Some(Subject {
        photo: photo.to_string(),
        make: facts.make,
        model: facts.model,
        iso: facts.iso,
        wb,
        offset_ev: dng_standard::offset_for_photo(Some(photo)),
    })
}

/// A look's file that is already beside a photo.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct LookFile {
    pub camera: String,
    pub look: String,
    pub path: String,
}

/// What the open photo can have.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct PhotoLooks {
    /// Its own camera on the list.
    pub camera: String,
    pub files: Vec<LookFile>,
}

/// What the provider offers.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct ProviderLooks {
    pub cameras: Vec<OfferedCamera>,
    pub looks: Vec<OfferedLook>,
    /// The switch.
    pub on: bool,
    /// None for no photo, a photo that is not a RAW, or one from no camera on
    /// the list: no provider looks show for it.
    pub photo: Option<PhotoLooks>,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct ProviderLooksArgs {
    pub path: Option<String>,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct ProviderLookMakeArgs {
    pub paths: Vec<String>,
    /// None for each photo's own camera.
    pub camera: Option<String>,
    pub look: String,
    pub remake: bool,
    pub front: bool,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct ProviderLookFileArgs {
    pub path: String,
    pub camera: String,
    pub look: String,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct ProviderLookCancelArgs {
    pub job_id: u64,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct ProviderLooksSwitchArgs {
    pub on: bool,
}

/// What the provider offers `path`; None when there is no provider.
pub fn looks(app: &AppHandle, path: Option<&str>) -> Option<ProviderLooks> {
    let provider = provider(app)?;
    let catalog = &provider.catalog;
    let photo = path.and_then(subject_of).and_then(|subject| {
        let camera = camera_for(catalog, None, &subject.make, &subject.model)?.to_string();
        let source = parse_virtual_path(&subject.photo).0;
        let files = catalog
            .cameras
            .iter()
            .flat_map(|c| catalog.looks.iter().map(move |l| (c, l)))
            .filter_map(|(c, l)| {
                let file = file_ops::look_file_for(&source, &c.id, &l.id)?;
                file.is_file().then(|| LookFile {
                    camera: c.id.clone(),
                    look: l.id.clone(),
                    path: file.to_string_lossy().into_owned(),
                })
            })
            .collect();
        Some(PhotoLooks { camera, files })
    });
    Some(ProviderLooks {
        cameras: catalog.cameras.clone(),
        looks: catalog.looks.clone(),
        on: is_on(profiles::library()?),
        photo,
    })
}

/// Queues `args`, reading each photo off the caller's thread. The job's id;
/// each photo's result comes as an [`EVENT`].
pub fn make(app: &AppHandle, args: ProviderLookMakeArgs) -> Result<u64, String> {
    let provider = provider(app).ok_or("no provider")?;
    let job = NEXT_JOB.fetch_add(1, Ordering::SeqCst);
    let app = app.clone();
    std::thread::spawn(move || {
        let report: Report = Arc::new(move |progress| {
            let _ = app.emit(EVENT, progress);
        });
        for path in args.paths {
            let request = subject_of(&path).and_then(|subject| {
                let chosen = args.camera.as_deref();
                let camera = camera_for(&provider.catalog, chosen, &subject.make, &subject.model)?;
                let listed = provider.catalog.looks.iter().any(|l| l.id == args.look);
                listed.then(|| Request {
                    camera: camera.to_string(),
                    look: args.look.clone(),
                    remake: args.remake,
                    front: args.front,
                    job,
                    report: Arc::clone(&report),
                    subject,
                })
            });
            match request {
                Some(request) => provider.queue.request(request),
                None => report(LookProgress {
                    job_id: job,
                    path,
                    camera: args.camera.clone().unwrap_or_default(),
                    look: args.look.clone(),
                    outcome: LookOutcome::Unsupported,
                }),
            }
        }
    });
    Ok(job)
}

/// Whether a photo has this look's file, with a provider or without one: the
/// file is all a look needs to show.
pub fn file_is_there(photo: &str, camera: &str, look: &str) -> bool {
    file_ops::look_file_for(&parse_virtual_path(photo).0, camera, look)
        .is_some_and(|file| file.is_file())
}

/// For an export: whether the look the edit names is there to render, made
/// now if the provider can. False means it renders without its look, and
/// `cancel` stops the wait. An edit with no provider look needs nothing.
pub fn look_file_ready(app: &AppHandle, js: &Value, photo: &str, cancel: &AtomicBool) -> bool {
    if js["lutPath"].as_str() != Some(provider_look::MARKER) {
        return true;
    }
    let Some(file) = provider_look::look_file(js, photo) else {
        return false;
    };
    if file.is_file() {
        return true;
    }
    let Ok(look) = provider_look::ProviderLook::deserialize(&js[provider_look::KEY]) else {
        return false;
    };
    let Some(provider) = provider(app) else {
        return false;
    };
    let listed = provider.catalog.cameras.iter().any(|c| c.id == look.camera)
        && provider.catalog.looks.iter().any(|l| l.id == look.look);
    let Some(subject) = subject_of(photo).filter(|_| listed) else {
        return false;
    };
    let made = provider
        .queue
        .wait_for(subject, look.camera, look.look, cancel);
    made == LookOutcome::Done
}

/// Takes a job's photos out of the queue.
pub fn cancel(job: u64) {
    if let Some(provider) = READY.get() {
        provider.queue.cancel(job);
    }
}

/// Keeps the switch. Waiting work belongs to the interface, which cancels it.
pub fn switch(on: bool) -> Result<(), String> {
    set_on(profiles::library_ready()?, on)
}

/// The app is closing.
pub fn stop_all() {
    if let Some(provider) = READY.get() {
        provider.queue.stop_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::{Receiver, channel};

    /// The fake provider: a real program, compiled once for this test run. What
    /// it does is its file name, so each behaviour is a copy under another
    /// name. It keeps a line per `lut` call in `calls.log` beside it.
    const FAKE_SOURCE: &str = r#"
use std::io::{Read, Write};
fn output_of(request: &str) -> String {
    let start = request.find("\"output\":\"").expect("output") + 10;
    let rest = &request[start..];
    let end = rest.find('"').expect("end");
    rest[..end].replace("\\\\", "\\")
}
fn main() {
    let exe = std::env::current_exe().unwrap();
    let dir = exe.parent().unwrap().to_path_buf();
    let kind = exe.file_stem().unwrap().to_str().unwrap().to_string();
    let word = std::env::args().nth(1).unwrap_or_default();
    let mut request = String::new();
    std::io::stdin().read_to_string(&mut request).unwrap();
    let mut calls = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("calls.log")).unwrap();
    writeln!(calls, "{request}").unwrap();
    drop(calls);
    if word == "list" {
        if kind == "crash" { std::process::exit(3); }
        if kind == "slowlist" { std::thread::sleep(std::time::Duration::from_secs(30)); }
        if kind == "badlist" { print!("not json"); return; }
        print!("{{\"protocol\":1,\"generator\":\"fake 1\",\"cameras\":[{{\"id\":\"cam1\",\"name\":\"Camera One\",\"make\":\"Acme\",\"model\":\"One\"}},{{\"id\":\"cam2\",\"name\":\"Camera Two\",\"make\":\"Acme\",\"model\":\"Two\"}}],\"looks\":[{{\"id\":\"look1\",\"name\":\"Invented Stock\",\"group\":\"Colour\"}},{{\"id\":\"look2\",\"name\":\"Other Stock\"}}]}}");
        return;
    }
    let output = output_of(&request);
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    match kind.as_str() {
        "good" => {
            std::fs::copy(dir.join("good.png"), &output).unwrap();
            write!(out, "{{\"ok\":true,\"seconds\":0.1}}").unwrap();
        }
        "paced" => {
            while !dir.join("go").exists() { std::thread::sleep(std::time::Duration::from_millis(5)); }
            std::fs::copy(dir.join("good.png"), &output).unwrap();
            write!(out, "{{\"ok\":true,\"seconds\":0.1}}").unwrap();
        }
        "crash" => { eprintln!("boom"); std::process::exit(3); }
        "ticking" => loop {
            let mut ticks = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("ticks.log")).unwrap();
            write!(ticks, ".").unwrap();
            drop(ticks);
            std::thread::sleep(std::time::Duration::from_millis(10));
        },
        "slow" => { std::fs::write(&output, "half").unwrap(); std::thread::sleep(std::time::Duration::from_secs(30)); }
        "badjson" => { std::fs::copy(dir.join("good.png"), &output).unwrap(); write!(out, "{{not json").unwrap(); }
        "noisy" => { std::fs::copy(dir.join("good.png"), &output).unwrap(); write!(out, "{}", "x".repeat(100_000)).unwrap(); }
        "badpng" => { std::fs::write(&output, "not a picture").unwrap(); write!(out, "{{\"ok\":true}}").unwrap(); }
        "smallpng" => { std::fs::copy(dir.join("small.png"), &output).unwrap(); write!(out, "{{\"ok\":true}}").unwrap(); }
        "hugepng" => { std::fs::write(&output, vec![0u8; 3_000_000]).unwrap(); write!(out, "{{\"ok\":true}}").unwrap(); }
        "nofile" => { write!(out, "{{\"ok\":true}}").unwrap(); }
        "unsupported" => write!(out, "{{\"ok\":false,\"error\":\"unsupported_camera\",\"message\":\"no\"}}").unwrap(),
        "unknown" => write!(out, "{{\"ok\":false,\"error\":\"unknown_look\"}}").unwrap(),
        "missing" => write!(out, "{{\"ok\":false,\"error\":\"data_missing\"}}").unwrap(),
        "strange" => write!(out, "{{\"ok\":false,\"error\":\"a_code_nobody_knows\"}}").unwrap(),
        other => panic!("{other}"),
    }
}
"#;

    /// Compiled once for this run and kept in its own temporary folder.
    fn fake_program() -> &'static Path {
        static BUILT: OnceLock<(tempfile::TempDir, PathBuf)> = OnceLock::new();
        let (_, exe) = BUILT.get_or_init(|| {
            let dir = tempfile::tempdir().expect("folder");
            let source = dir.path().join("fake.rs");
            std::fs::write(&source, FAKE_SOURCE).expect("source");
            let exe = dir
                .path()
                .join(format!("fake{}", std::env::consts::EXE_SUFFIX));
            let built = Command::new("rustc")
                .current_dir(env!("CARGO_MANIFEST_DIR"))
                .args(["--edition", "2024", "-C", "opt-level=0"])
                .args(["-C", "debuginfo=0"])
                .arg("-o")
                .arg(&exe)
                .arg(&source)
                .output()
                .expect("rustc");
            assert!(
                built.status.success(),
                "{}",
                String::from_utf8_lossy(&built.stderr)
            );
            (dir, exe)
        });
        exe
    }

    /// `kind` of the fake provider in a folder of its own, with a good PNG and
    /// a small one beside it. The folder goes with this.
    struct Fake {
        _dir: tempfile::TempDir,
        exe: PathBuf,
    }

    impl std::ops::Deref for Fake {
        type Target = Path;

        fn deref(&self) -> &Path {
            &self.exe
        }
    }

    fn fake(kind: &str) -> Fake {
        let dir = tempfile::tempdir().expect("folder");
        write_png(&dir.path().join("good.png"), 512);
        write_png(&dir.path().join("small.png"), 64);
        let exe = dir
            .path()
            .join(format!("{kind}{}", std::env::consts::EXE_SUFFIX));
        if std::fs::hard_link(fake_program(), &exe).is_err() {
            std::fs::copy(fake_program(), &exe).expect("copy");
        }
        Fake { _dir: dir, exe }
    }

    fn write_png(path: &Path, side: u32) {
        let mut png = image::RgbImage::new(side, side);
        for (i, pixel) in png.pixels_mut().enumerate() {
            *pixel = image::Rgb([(i % 251) as u8, (i % 241) as u8, (i % 239) as u8]);
        }
        png.save(path).expect("png");
    }

    /// For a test that means to run out of time.
    const QUICK: Limits = Limits {
        list: Duration::from_secs(1),
        lut: Duration::from_secs(1),
    };

    /// A new program can take seconds to start on a busy machine.
    const ROOMY: Limits = Limits {
        list: Duration::from_secs(60),
        lut: Duration::from_secs(60),
    };

    /// Under the 30 s the slow fakes sleep.
    const IN_TIME: Duration = Duration::from_secs(25);

    fn wait_for(what: &str, done: impl Fn() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(60);
        while !done() {
            assert!(Instant::now() < deadline, "{what}");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn calls(exe: &Path) -> usize {
        std::fs::read_to_string(exe.with_file_name("calls.log"))
            .map(|text| text.lines().count())
            .unwrap_or(0)
    }

    fn release(exe: &Path) {
        std::fs::write(exe.with_file_name("go"), "").expect("go");
    }

    // ------------------------------------------------------------ the setting

    fn library(text: Option<&str>) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("folder");
        if let Some(text) = text {
            std::fs::write(dir.path().join("argentum-processing.json"), text).expect("file");
        }
        dir
    }

    /// No file, no key, an empty value: every public user. Off, with nothing
    /// to validate and so nothing to run.
    #[test]
    fn absent_or_empty_means_off() {
        for text in [
            None,
            Some(r#"{ "tiffBitDepth": 8 }"#),
            Some(r#"{ "lutProvider": "" }"#),
            Some(r#"{ "lutProvider": "   " }"#),
            Some(r#"{ "lutProvider": 7 }"#),
        ] {
            assert_eq!(configured(library(text).path()), None, "{text:?}");
        }
    }

    #[test]
    fn the_program_is_read_from_the_file() {
        let lib = library(Some(r#"{ "lutProvider": "C:\\tools\\made-up.exe" }"#));
        assert_eq!(
            configured(lib.path()),
            Some(r"C:\tools\made-up.exe".to_string())
        );
    }

    /// Saving any other preference neither adds the field nor drops it.
    #[test]
    fn saving_other_preferences_leaves_the_field_alone() {
        let other: Setting<u8> = Setting::new("tiffBitDepth");

        let absent = library(None);
        other.save(absent.path(), &8).expect("save");
        SWITCH.save(absent.path(), &false).expect("save");
        let file = absent.path().join("argentum-processing.json");
        let text = std::fs::read_to_string(file).expect("file");
        assert!(!text.contains("lutProvider"), "{text}");

        let by_hand = library(Some(r#"{ "lutProvider": "C:\\tools\\made-up.exe" }"#));
        other.save(by_hand.path(), &8).expect("save");
        SWITCH.save(by_hand.path(), &false).expect("save");
        assert_eq!(
            configured(by_hand.path()),
            Some(r"C:\tools\made-up.exe".to_string())
        );
    }

    /// The field is read through one constant and written by nothing.
    #[test]
    fn nothing_here_can_write_the_field() {
        let source = include_str!("lut_provider.rs");
        let code = source.split("#[cfg(test)]").next().expect("code");
        assert!(!code.contains("PROGRAM.save"));
    }

    #[test]
    fn the_switch_starts_on_and_remembers() {
        let lib = library(Some(r#"{ "lutProvider": "C:\\tools\\made-up.exe" }"#));
        assert!(is_on(lib.path()));
        set_on(lib.path(), false).expect("save");
        assert!(!is_on(lib.path()));
    }

    /// So `providerLooksOn` can't appear for anyone without the field.
    #[test]
    fn the_switch_is_not_written_without_a_provider() {
        let lib = library(None);
        assert!(set_on(lib.path(), false).is_err());
        assert!(!lib.path().join("argentum-processing.json").exists());
    }

    // ------------------------------------------------------------- validation

    #[test]
    fn only_a_local_program_with_a_full_path_passes() {
        let exe = fake("good");
        assert_eq!(check_path(&exe.to_string_lossy()), Ok(exe.to_path_buf()));

        let dir = exe.parent().expect("folder");
        let name = exe.file_name().expect("name").to_string_lossy().to_string();
        let up = format!(
            "{}/../{}/{name}",
            dir.display(),
            dir.file_name().unwrap().to_string_lossy()
        );
        for bad in [
            name.as_str(),
            "tools/made-up.exe",
            up.as_str(),
            r"\\server\share\made-up.exe",
            "//server/share/made-up.exe",
        ] {
            assert!(check_path(bad).is_err(), "{bad}");
        }
        assert!(check_path(&dir.join("gone.exe").to_string_lossy()).is_err());
        assert!(check_path(&dir.to_string_lossy()).is_err(), "a folder");
    }

    #[test]
    #[cfg(windows)]
    fn on_windows_it_must_be_an_exe() {
        let exe = fake("good");
        let cmd = exe.with_extension("cmd");
        std::fs::write(&cmd, "@echo off").expect("file");
        assert!(check_path(&cmd.to_string_lossy()).is_err());
    }

    const GOOD_LIST: &str = r#"{ "protocol": 1, "generator": "t",
        "cameras": [{ "id": "cam1", "name": "Camera One", "make": "Acme", "model": "One" }],
        "looks": [{ "id": "look1", "name": "Invented Stock", "group": "Colour" }] }"#;

    #[test]
    fn a_good_list_is_kept() {
        let catalog = read_list(GOOD_LIST.as_bytes()).expect("a list");
        assert_eq!(catalog.cameras[0].id, "cam1");
        assert_eq!(catalog.looks[0].group.as_deref(), Some("Colour"));
    }

    #[test]
    fn a_list_of_another_protocol_is_refused() {
        let other = GOOD_LIST.replace("\"protocol\": 1", "\"protocol\": 2");
        assert!(read_list(other.as_bytes()).is_err());
        assert!(read_list(b"not json").is_err());
    }

    #[test]
    fn a_bad_entry_is_dropped_and_the_rest_stay() {
        let long = "x".repeat(81);
        let list = format!(
            r#"{{ "protocol": 1,
            "cameras": [
              {{ "id": "cam1", "name": "One", "make": "Acme", "model": "One" }},
              {{ "id": "cam1", "name": "Again", "make": "Acme", "model": "Again" }},
              {{ "id": "../x", "name": "Bad id", "make": "Acme", "model": "X" }},
              {{ "id": "cam3", "name": "", "make": "Acme", "model": "X" }},
              {{ "id": "cam4", "name": "{long}", "make": "Acme", "model": "X" }},
              {{ "id": "cam5", "name": "No make", "make": "", "model": "X" }},
              {{ "id": "cam6", "name": "No model", "make": "Acme", "model": "" }},
              {{ "id": 7 }}
            ],
            "looks": [
              {{ "id": "look1", "name": "Invented Stock" }},
              {{ "id": "look1", "name": "Again" }},
              {{ "id": "Look2", "name": "Upper case" }},
              {{ "id": "look3", "name": "" }}
            ] }}"#
        );
        let catalog = read_list(list.as_bytes()).expect("a list");
        assert_eq!(catalog.cameras.len(), 1);
        assert_eq!(catalog.looks.len(), 1);
    }

    #[test]
    fn with_no_camera_or_no_look_left_the_provider_is_off() {
        let no_cameras =
            r#"{ "protocol": 1, "cameras": [], "looks": [{ "id": "a", "name": "A" }] }"#;
        let no_looks = r#"{ "protocol": 1, "looks": [],
            "cameras": [{ "id": "c", "name": "C", "make": "M", "model": "N" }] }"#;
        assert!(read_list(no_cameras.as_bytes()).is_err());
        assert!(read_list(no_looks.as_bytes()).is_err());
    }

    #[test]
    fn a_working_program_lists_its_cameras_and_looks() {
        let exe = fake("good");
        let catalog = validate(&exe.to_string_lossy(), &ROOMY).expect("a list");
        assert_eq!(catalog.cameras.len(), 2);
        assert_eq!(catalog.looks.len(), 2);
    }

    /// The fake counts its `list` calls, so this would see one that ran.
    #[test]
    fn with_the_field_absent_or_empty_no_process_runs() {
        let exe = fake("good");
        let named = format!(r#"{{ "lutProvider": {:?} }}"#, exe.to_string_lossy());
        assert!(offered(library(Some(&named)).path(), &ROOMY).is_some());
        assert_eq!(calls(&exe), 1, "the fake was not counted");

        for text in [None, Some(r#"{ "lutProvider": "" }"#)] {
            assert_eq!(offered(library(text).path(), &ROOMY), None);
            assert_eq!(calls(&exe), 1, "{text:?}");
        }
    }

    #[test]
    fn a_program_that_crashes_is_slow_or_lies_is_refused() {
        for kind in ["crash", "slowlist", "badlist"] {
            let exe = fake(kind);
            let started = Instant::now();
            assert!(validate(&exe.to_string_lossy(), &QUICK).is_err(), "{kind}");
            assert!(started.elapsed() < IN_TIME, "{kind}");
        }
    }

    // -------------------------------------------------------------- one LUT

    fn subject(photo: &str) -> Subject {
        Subject {
            photo: photo.to_string(),
            make: "Acme".to_string(),
            model: "One".to_string(),
            iso: Some(400.0),
            wb: [1.8, 1.0, 1.0, 1.6],
            offset_ev: 0.25,
        }
    }

    /// A three-colour photo's white balance arrives as R, G, B, NaN, and that
    /// NaN does not mean the photo has none.
    #[test]
    fn a_three_colour_white_balance_reaches_the_program_with_green_twice() {
        for [r, g, b] in [[1.8, 1.0, 1.6], [1.5, 1.0, 2.25]] {
            assert_eq!(wb_for_program([r, g, b, f32::NAN]), Some([r, g, g, b]));
        }
    }

    #[test]
    fn a_photo_with_no_white_balance_is_refused() {
        assert_eq!(wb_for_program([f32::NAN; 4]), None);
        assert_eq!(wb_for_program([1.8, f32::NAN, 1.6, 1.0]), None);
    }

    /// Four real numbers are not reshaped; the default for an unread file is
    /// four ones.
    #[test]
    fn four_real_numbers_pass_unchanged() {
        assert_eq!(wb_for_program([1.0; 4]), Some([1.0; 4]));
        let four = [1.8, 1.0, 1.0, 1.6];
        assert_eq!(wb_for_program(four), Some(four));
    }

    /// What a run of `kind` gives, and whether it left its scratch file.
    fn make_with(kind: &str) -> (bool, Result<(Vec<u8>, Lut), Failure>) {
        let limits = if kind == "slow" { QUICK } else { ROOMY };
        let exe = fake(kind);
        let scratch = exe.with_file_name("scratch.png");
        let got = make_lut(
            &exe,
            &limits,
            &subject("/photos/IMG.CR3"),
            "cam1",
            "look1",
            &scratch,
            &AtomicBool::new(false),
        );
        (scratch.exists(), got)
    }

    #[test]
    fn a_good_run_gives_a_checked_lut_and_clears_the_scratch_file() {
        let (left, got) = make_with("good");
        let (bytes, lut) = got.expect("a LUT");
        assert_eq!(lut.size, 64);
        assert!(!bytes.is_empty());
        assert!(!left);
    }

    #[test]
    fn the_request_names_what_the_protocol_says() {
        let exe = fake("good");
        let scratch = exe.with_file_name("scratch.png");
        make_lut(
            &exe,
            &ROOMY,
            &subject("/photos/IMG.CR3"),
            "cam1",
            "look1",
            &scratch,
            &AtomicBool::new(false),
        )
        .expect("a LUT");
        let line = std::fs::read_to_string(exe.with_file_name("calls.log")).expect("calls");
        let sent: Value = serde_json::from_str(line.trim()).expect("JSON");
        assert_eq!(sent["protocol"], 1);
        assert_eq!(sent["camera"], "cam1");
        assert_eq!(sent["look"], "look1");
        assert_eq!(sent["photo"]["path"], "/photos/IMG.CR3");
        assert_eq!(sent["photo"]["make"], "Acme");
        assert_eq!(sent["photo"]["model"], "One");
        assert_eq!(sent["photo"]["iso"], 400.0);
        assert_eq!(sent["asShotWb"].as_array().expect("WB").len(), 4);
        assert_eq!(sent["rendering"]["mode"], "linear");
        assert_eq!(sent["rendering"]["offsetEv"], 0.25);
        assert_eq!(sent["grid"], 64);
        assert_eq!(sent["output"], scratch.to_string_lossy().as_ref());
    }

    #[test]
    fn a_crash_is_a_crash() {
        let (left, got) = make_with("crash");
        assert_eq!(got.err(), Some(Failure::Crash));
        assert!(!left);
    }

    #[test]
    fn a_timeout_kills_the_run_and_clears_the_scratch_file() {
        let started = Instant::now();
        let (left, got) = make_with("slow");
        assert_eq!(got.err(), Some(Failure::Timeout));
        assert!(started.elapsed() < IN_TIME);
        assert!(!left);
    }

    #[test]
    fn a_killed_run_stops_working() {
        let exe = fake("ticking");
        let scratch = exe.with_file_name("scratch.png");
        let ticks = exe.with_file_name("ticks.log");
        let cancel = Arc::new(AtomicBool::new(false));
        let run = {
            let (exe, cancel) = (exe.to_path_buf(), Arc::clone(&cancel));
            std::thread::spawn(move || {
                let subject = subject("/photos/IMG.CR3");
                make_lut(&exe, &ROOMY, &subject, "cam1", "look1", &scratch, &cancel)
            })
        };
        wait_for("it never ran", || ticks.exists());
        cancel.store(true, Ordering::SeqCst);
        assert_eq!(run.join().expect("run").err(), Some(Failure::Cancelled));
        let size = || std::fs::metadata(&ticks).expect("ticks").len();
        let at_the_end = size();
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(size(), at_the_end);
    }

    #[test]
    fn bad_json_counts_as_a_crash() {
        let (left, got) = make_with("badjson");
        assert_eq!(got.err(), Some(Failure::Crash));
        assert!(!left);
    }

    #[test]
    fn too_much_output_counts_as_a_crash() {
        let (_, got) = make_with("noisy");
        assert_eq!(got.err(), Some(Failure::Crash));
    }

    #[test]
    fn a_bad_png_is_refused() {
        for kind in ["badpng", "smallpng", "hugepng", "nofile"] {
            let (left, got) = make_with(kind);
            assert_eq!(got.err(), Some(Failure::BadFile), "{kind}");
            assert!(!left, "{kind}");
        }
    }

    #[test]
    fn a_reported_failure_keeps_its_code_and_an_unknown_one_is_internal() {
        for (kind, want) in [
            ("unsupported", Refusal::UnsupportedCamera),
            ("unknown", Refusal::UnknownLook),
            ("missing", Refusal::DataMissing),
            ("strange", Refusal::Internal),
        ] {
            let (_, got) = make_with(kind);
            assert_eq!(got.err(), Some(Failure::Refused(want)), "{kind}");
        }
    }

    // ---------------------------------------------------------------- the queue

    /// Everything a queue reports, in order.
    struct Heard {
        queue: Arc<Queue>,
        told: Receiver<LookProgress>,
        report: Report,
    }

    fn queue_of(exe: &Path) -> Heard {
        let (tell, told) = channel();
        let tell = Mutex::new(tell);
        Heard {
            queue: Queue::new(
                exe.to_path_buf(),
                exe.with_file_name("scratch"),
                ROOMY,
                Box::new(|_, _, _| {}),
            ),
            told,
            report: Arc::new(move |progress| {
                let _ = tell.lock().unwrap().send(progress);
            }),
        }
    }

    impl Heard {
        fn ask(&self, photo: &Path, job: u64) {
            self.ask_for(photo, "look1", job, false, false);
        }

        fn ask_for(&self, photo: &Path, look: &str, job: u64, remake: bool, front: bool) {
            self.queue.request(Request {
                subject: subject(&photo.to_string_lossy()),
                camera: "cam1".to_string(),
                look: look.to_string(),
                remake,
                front,
                job,
                report: Arc::clone(&self.report),
            });
        }

        fn next(&self) -> LookProgress {
            self.told
                .recv_timeout(Duration::from_secs(60))
                .expect("a report")
        }
    }

    /// A photo in the fake's folder; nothing reads it.
    fn photo_beside(exe: &Path, name: &str) -> PathBuf {
        exe.with_file_name(name)
    }

    fn look_file_of_photo(photo: &Path) -> PathBuf {
        file_ops::look_file_for(photo, "cam1", "look1").expect("a name")
    }

    #[test]
    fn the_file_lands_beside_the_photo_and_is_reported() {
        let exe = fake("good");
        let heard = queue_of(&exe);
        let photo = photo_beside(&exe, "IMG.CR3");
        heard.ask(&photo, 1);
        let told = heard.next();
        assert_eq!(told.outcome, LookOutcome::Done);
        assert_eq!(told.job_id, 1);
        assert_eq!(told.path, photo.to_string_lossy());
        assert_eq!(
            (told.camera.as_str(), told.look.as_str()),
            ("cam1", "look1")
        );
        assert!(look_file_of_photo(&photo).is_file());
    }

    #[test]
    fn a_valid_file_already_there_is_used_with_no_call() {
        let exe = fake("good");
        let heard = queue_of(&exe);
        let photo = photo_beside(&exe, "IMG.CR3");
        write_png(&look_file_of_photo(&photo), 512);
        heard.ask(&photo, 1);
        assert_eq!(heard.next().outcome, LookOutcome::Done);
        assert_eq!(calls(&exe), 0);
    }

    #[test]
    fn a_broken_file_already_there_is_made_again() {
        let exe = fake("good");
        let heard = queue_of(&exe);
        let photo = photo_beside(&exe, "IMG.CR3");
        std::fs::write(look_file_of_photo(&photo), "half").expect("file");
        heard.ask(&photo, 1);
        assert_eq!(heard.next().outcome, LookOutcome::Done);
        assert_eq!(calls(&exe), 1);
    }

    #[test]
    fn remake_overwrites_the_file_and_replaces_the_cached_lut() {
        let exe = fake("good");
        let (tell, placed) = channel();
        let tell = Mutex::new(tell);
        let queue = Queue::new(
            exe.to_path_buf(),
            exe.with_file_name("scratch"),
            ROOMY,
            Box::new(move |_, file, lut| {
                let _ = tell.lock().unwrap().send((file.to_path_buf(), lut.size));
            }),
        );
        let (reply, replied) = channel();
        let reply = Mutex::new(reply);
        let photo = photo_beside(&exe, "IMG.CR3");
        write_png(&look_file_of_photo(&photo), 64);
        queue.request(Request {
            subject: subject(&photo.to_string_lossy()),
            camera: "cam1".to_string(),
            look: "look1".to_string(),
            remake: true,
            front: false,
            job: 1,
            report: Arc::new(move |progress| {
                let _ = reply.lock().unwrap().send(progress);
            }),
        });
        let told = replied
            .recv_timeout(Duration::from_secs(60))
            .expect("a report");
        assert_eq!(told.outcome, LookOutcome::Done);
        assert_eq!(calls(&exe), 1);
        assert_eq!(
            placed.recv_timeout(Duration::from_secs(1)).expect("placed"),
            (look_file_of_photo(&photo), 64)
        );
        assert!(checked_file(&std::fs::read(look_file_of_photo(&photo)).expect("file")).is_ok());
    }

    #[test]
    fn frames_with_the_same_settings_share_one_run() {
        let exe = fake("paced");
        let heard = queue_of(&exe);
        let photos: Vec<PathBuf> = (0..5)
            .map(|n| photo_beside(&exe, &format!("IMG_{n}.CR3")))
            .collect();
        for photo in &photos {
            heard.ask(photo, 1);
        }
        release(&exe);
        for _ in &photos {
            assert_eq!(heard.next().outcome, LookOutcome::Done);
        }
        assert_eq!(calls(&exe), 1);
        for photo in &photos {
            assert!(look_file_of_photo(photo).is_file());
        }
    }

    #[test]
    fn a_double_click_does_no_extra_work() {
        let exe = fake("paced");
        let heard = queue_of(&exe);
        let photo = photo_beside(&exe, "IMG.CR3");
        heard.ask(&photo, 1);
        heard.ask(&photo, 2);
        release(&exe);
        assert_eq!(heard.next().outcome, LookOutcome::Done);
        assert_eq!(calls(&exe), 1);
    }

    #[test]
    fn two_run_at_a_time() {
        let exe = fake("paced");
        let heard = queue_of(&exe);
        for n in 0..4 {
            let mut other = subject(&photo_beside(&exe, &format!("IMG_{n}.CR3")).to_string_lossy());
            other.wb[0] += n as f32;
            heard.queue.request(Request {
                subject: other,
                camera: "cam1".to_string(),
                look: "look1".to_string(),
                remake: false,
                front: false,
                job: 1,
                report: Arc::clone(&heard.report),
            });
        }
        wait_for("two runs", || calls(&exe) >= 2);
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(calls(&exe), 2);
        release(&exe);
        for _ in 0..4 {
            assert_eq!(heard.next().outcome, LookOutcome::Done);
        }
        assert_eq!(calls(&exe), 4);
    }

    #[test]
    fn the_open_photo_goes_to_the_front() {
        let exe = fake("paced");
        let heard = queue_of(&exe);
        // Two fill the runs; of the two waiting, the last asked goes first.
        for n in 0..3 {
            let mut other = subject(&photo_beside(&exe, &format!("IMG_{n}.CR3")).to_string_lossy());
            other.wb[0] += n as f32;
            heard.queue.request(Request {
                subject: other,
                camera: "cam1".to_string(),
                look: "look1".to_string(),
                remake: false,
                front: false,
                job: 1,
                report: Arc::clone(&heard.report),
            });
        }
        let open = photo_beside(&exe, "OPEN.CR3");
        heard.ask_for(&open, "look2", 2, false, true);
        let next = {
            let state = heard.queue.state();
            let photos = state.waiting.iter().map(|item| item.subject.photo.clone());
            photos.collect::<Vec<_>>()
        };
        assert_eq!(next.len(), 2);
        assert_eq!(next[0], open.to_string_lossy());
        release(&exe);
        for _ in 0..4 {
            assert_eq!(heard.next().outcome, LookOutcome::Done);
        }
    }

    #[test]
    fn cancelling_while_others_wait_lets_the_run_go_on() {
        let exe = fake("paced");
        let heard = queue_of(&exe);
        let (first, second) = (photo_beside(&exe, "A.CR3"), photo_beside(&exe, "B.CR3"));
        heard.ask(&first, 1);
        heard.ask(&second, 2);
        heard.queue.cancel(1);
        assert_eq!(heard.next().outcome, LookOutcome::Cancelled);
        release(&exe);
        let told = heard.next();
        assert_eq!(told.outcome, LookOutcome::Done);
        assert_eq!(told.path, second.to_string_lossy());
        assert!(!look_file_of_photo(&first).exists());
        assert!(look_file_of_photo(&second).exists());
    }

    #[test]
    fn cancelling_the_last_photo_kills_the_run_and_clears_its_scratch_file() {
        let exe = fake("slow");
        let heard = queue_of(&exe);
        let photo = photo_beside(&exe, "IMG.CR3");
        heard.ask(&photo, 1);
        let scratch = exe.with_file_name("scratch");
        let files = || std::fs::read_dir(&scratch).expect("folder").count();
        wait_for("the run left its half file", || files() == 1);
        heard.queue.cancel(1);
        assert_eq!(heard.next().outcome, LookOutcome::Cancelled);
        wait_for("the run stopped", || !heard.queue.is_busy());
        assert_eq!(files(), 0);
        assert!(!look_file_of_photo(&photo).exists());
    }

    #[test]
    fn another_look_on_the_same_photo_replaces_the_waiting_one() {
        let exe = fake("paced");
        let heard = queue_of(&exe);
        let photo = photo_beside(&exe, "IMG.CR3");
        heard.ask_for(&photo, "look1", 1, false, false);
        heard.ask_for(&photo, "look2", 2, false, false);
        assert_eq!(heard.next().outcome, LookOutcome::Cancelled);
        release(&exe);
        let told = heard.next();
        assert_eq!(
            (told.look.as_str(), told.outcome),
            ("look2", LookOutcome::Done)
        );
        assert!(!look_file_of_photo(&photo).exists());
    }

    #[test]
    fn two_crashes_in_a_row_turn_the_provider_off() {
        let exe = fake("crash");
        let heard = queue_of(&exe);
        heard.ask(&photo_beside(&exe, "A.CR3"), 1);
        assert_eq!(heard.next().outcome, LookOutcome::Failed);
        assert!(!heard.queue.is_off());
        heard.ask_for(&photo_beside(&exe, "B.CR3"), "look2", 2, false, false);
        assert_eq!(heard.next().outcome, LookOutcome::Failed);
        assert!(heard.queue.is_off());
        heard.ask(&photo_beside(&exe, "C.CR3"), 3);
        assert_eq!(heard.next().outcome, LookOutcome::Failed);
        assert_eq!(calls(&exe), 2);
    }

    #[test]
    fn a_failed_item_is_not_tried_again_on_its_own() {
        let exe = fake("badpng");
        let heard = queue_of(&exe);
        heard.ask(&photo_beside(&exe, "A.CR3"), 1);
        assert_eq!(heard.next().outcome, LookOutcome::Failed);
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(calls(&exe), 1);
    }

    #[test]
    fn a_refused_camera_is_not_asked_again_this_session() {
        let exe = fake("unsupported");
        let heard = queue_of(&exe);
        heard.ask(&photo_beside(&exe, "A.CR3"), 1);
        assert_eq!(heard.next().outcome, LookOutcome::Unsupported);
        heard.ask(&photo_beside(&exe, "B.CR3"), 2);
        assert_eq!(heard.next().outcome, LookOutcome::Unsupported);
        assert_eq!(calls(&exe), 1);
    }

    #[test]
    fn a_folder_that_cannot_be_written_stops_before_any_call() {
        let exe = fake("good");
        let heard = queue_of(&exe);
        let photo = exe.with_file_name("no-such-folder").join("IMG.CR3");
        heard.ask(&photo, 1);
        assert_eq!(heard.next().outcome, LookOutcome::Folder);
        assert_eq!(calls(&exe), 0);
    }

    #[test]
    fn a_closing_app_leaves_no_run_behind() {
        let exe = fake("slow");
        let heard = queue_of(&exe);
        heard.ask(&photo_beside(&exe, "IMG.CR3"), 1);
        wait_for("the run started", || calls(&exe) == 1);
        heard.queue.stop_all();
        assert_eq!(heard.next().outcome, LookOutcome::Cancelled);
        wait_for("the run stopped", || !heard.queue.is_busy());
    }

    #[test]
    fn an_export_waits_for_the_file() {
        let exe = fake("good");
        let heard = queue_of(&exe);
        let photo = photo_beside(&exe, "IMG.CR3");
        let never = AtomicBool::new(false);
        let got = heard.queue.wait_for(
            subject(&photo.to_string_lossy()),
            "cam1".to_string(),
            "look1".to_string(),
            &never,
        );
        assert_eq!(got, LookOutcome::Done);
        assert!(look_file_of_photo(&photo).is_file());
    }

    #[test]
    fn an_export_gets_on_when_the_look_fails() {
        let exe = fake("crash");
        let heard = queue_of(&exe);
        let photo = photo_beside(&exe, "IMG.CR3");
        let never = AtomicBool::new(false);
        let got = heard.queue.wait_for(
            subject(&photo.to_string_lossy()),
            "cam1".to_string(),
            "look1".to_string(),
            &never,
        );
        assert_eq!(got, LookOutcome::Failed);
    }

    #[test]
    fn a_cancelled_export_stops_waiting_and_kills_the_run() {
        let exe = fake("slow");
        let heard = queue_of(&exe);
        let photo = photo_beside(&exe, "IMG.CR3");
        let token = Arc::new(AtomicBool::new(false));
        let waiting = {
            let (queue, token) = (Arc::clone(&heard.queue), Arc::clone(&token));
            let subject = subject(&photo.to_string_lossy());
            std::thread::spawn(move || {
                queue.wait_for(subject, "cam1".to_string(), "look1".to_string(), &token)
            })
        };
        wait_for("the run started", || calls(&exe) == 1);
        token.store(true, Ordering::SeqCst);
        assert_eq!(waiting.join().expect("wait"), LookOutcome::Cancelled);
        wait_for("the run stopped", || !heard.queue.is_busy());
    }

    /// The open photo and its export can wait for the same file.
    #[test]
    fn everyone_waiting_for_a_photo_hears() {
        let exe = fake("paced");
        let heard = queue_of(&exe);
        let photo = photo_beside(&exe, "IMG.CR3");
        heard.ask(&photo, 1);
        let export = {
            let queue = Arc::clone(&heard.queue);
            let subject = subject(&photo.to_string_lossy());
            std::thread::spawn(move || {
                let never = AtomicBool::new(false);
                queue.wait_for(subject, "cam1".to_string(), "look1".to_string(), &never)
            })
        };
        wait_for("both wait", || {
            heard
                .queue
                .state()
                .items()
                .any(|item| item.waiters.len() == 2)
        });
        release(&exe);
        assert_eq!(heard.next().outcome, LookOutcome::Done);
        assert_eq!(export.join().expect("wait"), LookOutcome::Done);
        assert_eq!(calls(&exe), 1);
    }

    #[test]
    fn a_look_file_is_found_for_a_photo_and_its_virtual_copy() {
        let exe = fake("good");
        let photo = photo_beside(&exe, "IMG.CR3");
        let name = photo.to_string_lossy();
        assert!(!file_is_there(&name, "cam1", "look1"));
        write_png(&look_file_of_photo(&photo), 512);
        assert!(file_is_there(&name, "cam1", "look1"));
        assert!(file_is_there(&format!("{name}?vc=a1b2c3"), "cam1", "look1"));
        assert!(!file_is_there(&name, "cam2", "look1"));
        assert!(!file_is_there(&name, "../cam1", "look1"));
    }

    #[test]
    fn leftovers_are_cleared_at_first_use() {
        let exe = fake("good");
        let scratch = exe.with_file_name("scratch");
        std::fs::create_dir_all(&scratch).expect("folder");
        std::fs::write(scratch.join("old.png"), "left").expect("file");
        let _heard = queue_of(&exe);
        assert_eq!(std::fs::read_dir(&scratch).expect("folder").count(), 0);
    }

    // ---------------------------------------------------------- which camera

    fn catalog() -> Catalog {
        let camera = |id: &str, model: &str| OfferedCamera {
            id: id.to_string(),
            name: model.to_string(),
            make: "Acme".to_string(),
            model: model.to_string(),
        };
        Catalog {
            cameras: vec![camera("cam1", "One"), camera("cam2", "Two")],
            looks: vec![OfferedLook {
                id: "look1".to_string(),
                name: "Invented Stock".to_string(),
                group: None,
            }],
        }
    }

    #[test]
    fn a_photo_finds_its_camera() {
        assert_eq!(camera_for(&catalog(), None, "Acme", "Two"), Some("cam2"));
        assert_eq!(camera_for(&catalog(), None, "ACME", "two"), Some("cam2"));
    }

    #[test]
    fn a_photo_from_no_camera_on_the_list_gets_none() {
        assert_eq!(camera_for(&catalog(), None, "Acme", "Three"), None);
        assert_eq!(camera_for(&catalog(), None, "Other", "One"), None);
        assert_eq!(camera_for(&catalog(), Some("cam1"), "Acme", "Three"), None);
    }

    #[test]
    fn a_chosen_camera_stays_while_it_is_on_the_list() {
        assert_eq!(
            camera_for(&catalog(), Some("cam1"), "Acme", "Two"),
            Some("cam1")
        );
        assert_eq!(
            camera_for(&catalog(), Some("gone"), "Acme", "Two"),
            Some("cam2")
        );
    }
}
