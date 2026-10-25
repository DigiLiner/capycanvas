#[path = "../src/photo_reader.rs"]
mod photo_reader;
#[path = "../src/photo_reader/sandbox.rs"]
mod sandbox;

use layer_color::photo::{DecodeLimits, DecodedPhoto, DeliveryMetadata};
use layer_core::color::{ColorProfile, SampleDepth, source::{SourceChannels, SourceInterpretation}};
use std::{fs::{self, File}, io::{Cursor, Read, Write}, os::fd::AsRawFd, path::{Path, PathBuf},
    process::{Child, ChildStdout, Command, Stdio}, sync::{Arc, atomic::{AtomicBool, AtomicI32, Ordering}}, time::{Duration, Instant}};

const HELPER: &str = env!("CARGO_BIN_EXE_capy-photo-reader");
const TIMEOUT: Duration = Duration::from_secs(30);
const INVALID_FLAGS: usize = 101;
const TRUNCATED: usize = 102;
const EXCESSIVE: usize = 103;
const HANG: usize = 104;
const NO_SOURCE: usize = 105;

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let base = std::env::var_os("TMPDIR").or_else(|| std::env::var_os("CAPY_STORAGE_DIR"))
            .map(PathBuf::from).unwrap_or_else(|| if Path::new("/mnt/localssd").is_dir() {
                PathBuf::from("/mnt/localssd")
            } else { std::env::temp_dir() });
        let path = base.join(format!("photo-reader-{}-{}", std::process::id(), layer_core::PortableId::random()));
        assert!(path.is_absolute());
        fs::create_dir(&path).unwrap();
        layer_core::temp_files::set_directory(path.clone()).unwrap();
        Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) { fs::remove_dir_all(&self.0).unwrap(); }
}

fn limits() -> DecodeLimits {
    DecodeLimits { source_bytes: 64 << 20, codec_bytes: 768 << 20, dimension: 32768 }
}

fn compare(expected: &DecodedPhoto, actual: &DecodedPhoto) {
    assert_eq!(actual.source.extent, expected.source.extent);
    assert_eq!(actual.source.resolution, expected.source.resolution);
    assert_eq!(actual.source.interpretation, expected.source.interpretation);
    assert_eq!(actual.metadata, expected.metadata);
    assert_eq!(actual.first_frame, expected.first_frame);
    assert_eq!(actual.primary_image, expected.primary_image);
    let mut left = expected.source.rows();
    let mut right = actual.source.rows();
    let mut a = vec![0; expected.source.row_bytes()];
    let mut b = vec![0; actual.source.row_bytes()];
    for y in 0..expected.source.extent[1] {
        left.read(y, &mut a).unwrap(); right.read(y, &mut b).unwrap();
        assert_eq!(a, b, "row {y}");
    }
}

fn formats() {
    let interpretation = SourceInterpretation { channels: SourceChannels::Rgb, depth: SampleDepth::U8,
        profile: ColorProfile::default(), profile_assumed: false };
    let metadata = DeliveryMetadata::resolution(Some(layer_core::ImageResolution::ppi(300)));
    let row = |y: u32, bytes: &mut [u8]| {
        for (x, pixel) in bytes.as_chunks_mut::<3>().0.iter_mut().enumerate() {
            *pixel = [(x * 13) as u8, (y * 29) as u8, (x as u32 + y * 7) as u8];
        }
        Ok(())
    };
    let mut png = Vec::new();
    layer_color::photo::write_png_rows(&mut png, [17, 9], &interpretation, &metadata, row).unwrap();
    let mut jpeg = Vec::new();
    layer_color::photo::write_jpeg_rows(&mut jpeg, [17, 9], &interpretation, &metadata,
        layer_color::photo::JpegEncodeOptions { quality: 97, codec_bytes: 16 << 20 }, row).unwrap();
    let mut tiff = Cursor::new(Vec::new());
    layer_color::photo::write_tiff_rows(&mut tiff, [17, 9], &interpretation, &metadata, row).unwrap();
    let mut gif = b"GIF89a\x01\0\x01\0\x80\0\0\0\0\0\xff\0\0".to_vec();
    for _ in 0..2 { gif.extend_from_slice(b",\0\0\0\0\x01\0\x01\0\0\x02\x02\x44\x01\0"); }
    gif.push(b';');
    let fixtures: &[(&str, &[u8])] = &[
        ("PNG", &png), ("JPEG", &jpeg), ("TIFF", tiff.get_ref()), ("animated GIF", &gif),
        ("AVIF 12-bit ICC", include_bytes!("../../../crates/layer-color/tests/fixtures/avif/p3-12bit-icc.avif")),
        ("AVIF crop/rotation", include_bytes!("../../../crates/layer-color/tests/fixtures/avif/p3-10bit-crop-r1-m1.avif")),
        ("AVIF HDR", include_bytes!("../../../crates/layer-color/tests/fixtures/avif/hdr-rgb.avif")),
        ("HEIF", include_bytes!("../../../crates/layer-color/tests/fixtures/heif/flat-red-8bit.heic")),
    ];
    for (name, bytes) in fixtures {
        let expected = layer_color::photo::read_photo_detailed(Cursor::new(*bytes), limits()).unwrap();
        let actual = photo_reader::decode(Cursor::new(*bytes), limits(), &AtomicBool::new(false), Path::new(HELPER), TIMEOUT)
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        compare(&expected, &actual);
        println!("PASS {name}: source rows, interpretation, density, metadata and frame flags");
    }
    let expected = layer_color::photo::read_photo_detailed(Cursor::new(&png), limits()).unwrap();
    compare(&expected, &photo_reader::read(Cursor::new(&png), limits(), &AtomicBool::new(false)).unwrap());
    println!("PASS installed helper discovery");
    for bounded in [DecodeLimits { source_bytes: 1, ..limits() },
        DecodeLimits { codec_bytes: 1, ..limits() }, DecodeLimits { dimension: 8, ..limits() }] {
        assert!(photo_reader::decode(Cursor::new(&png), bounded, &AtomicBool::new(false),
            Path::new(HELPER), TIMEOUT).is_err(), "caller limits: {bounded:?}");
    }
    println!("PASS caller source, codec and dimension limits");
}

fn fake_helper(mode: usize) {
    let mut output = std::io::stdout().lock();
    match mode {
        INVALID_FLAGS => output.write_all(&[255]).unwrap(),
        TRUNCATED => output.write_all(b"\0PK\x03\x04truncated").unwrap(),
        EXCESSIVE => {
            output.write_all(&[0]).unwrap();
            let bound = layer_core::color::source::MAX_PROFILE_BYTES + layer_core::PhotoMetadata::MAX_BYTES
                + layer_core::ProjectLimits::default().metadata_bytes as usize + mode;
            let chunk = [0; 64 * 1024];
            for _ in 0..=bound / chunk.len() {
                if output.write_all(&chunk).is_err() { return; }
            }
        }
        HANG => {
            output.write_all(&[0]).unwrap(); output.flush().unwrap();
            std::thread::sleep(Duration::from_secs(60));
        }
        NO_SOURCE => {
            let document = layer_core::Document::new(layer_core::PortableId::random(), 17, 9,
                layer_core::DocumentNames { paint: "Photo".into(), paper: "Paper".into() });
            let editor = layer_core::Editor::new(document);
            let capture = editor.capture(1, Default::default()).unwrap();
            let cancel = AtomicBool::new(false);
            let package = layer_core::package::codec::PreparedPackage::prepare(&capture, None, &cancel).unwrap();
            output.write_all(&[0]).unwrap(); package.write(&mut output, &cancel).unwrap();
        }
        _ => panic!("unknown test helper mode {mode}"),
    }
}

fn failures(scratch: &Path) {
    let cancel = AtomicBool::new(false);
    assert!(photo_reader::decode(Cursor::new(b"not a photo"), limits(), &cancel, Path::new(HELPER), TIMEOUT).is_err());
    let error = photo_reader::decode(Cursor::new([]), limits(), &cancel, &scratch.join("absent-helper"), TIMEOUT)
        .err().unwrap();
    assert!(error.contains("unavailable"), "{error}");
    for mode in [INVALID_FLAGS, TRUNCATED, EXCESSIVE, NO_SOURCE] {
        let start = Instant::now();
        let error = photo_reader::decode(Cursor::new([0]), DecodeLimits { source_bytes: mode, ..limits() },
            &cancel, &std::env::current_exe().unwrap(), TIMEOUT).err().unwrap();
        assert!(start.elapsed() < TIMEOUT, "mode {mode}: {error}");
        if mode == NO_SOURCE { assert!(error.contains("returned no image"), "{error}"); }
        println!("PASS rejected helper output {mode}: {error}");
    }
    let error = photo_reader::decode(Cursor::new([]), limits(), &AtomicBool::new(true), Path::new(HELPER), TIMEOUT)
        .err().unwrap();
    assert!(error.contains("cancelled"), "{error}");
    println!("PASS malformed input, missing helper and pre-canceled job");
}

fn interruption(cancel_during_job: bool) {
    let cancelled = Arc::new(AtomicBool::new(false));
    let pid = Arc::new(AtomicI32::new(0));
    let tid = unsafe { libc::syscall(libc::SYS_gettid) };
    let children = format!("/proc/self/task/{tid}/children");
    let observer = {
        let cancelled = cancelled.clone(); let pid = pid.clone();
        std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(5);
            while Instant::now() < deadline {
                if let Some(child) = fs::read_to_string(&children).unwrap().split_whitespace().next() {
                    pid.store(child.parse().unwrap(), Ordering::Release);
                    if cancel_during_job {
                        std::thread::sleep(Duration::from_millis(30));
                        cancelled.store(true, Ordering::Release);
                    }
                    return;
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            panic!("helper never started");
        })
    };
    let start = Instant::now();
    let timeout = if cancel_during_job { TIMEOUT } else { Duration::from_millis(250) };
    let error = photo_reader::decode(Cursor::new([0]), DecodeLimits { source_bytes: HANG, ..limits() },
        &cancelled, &std::env::current_exe().unwrap(), timeout).err().unwrap();
    observer.join().unwrap();
    assert!(start.elapsed() < Duration::from_secs(5), "{error}");
    assert!(error.contains(if cancel_during_job { "cancelled" } else { "timed out" }), "{error}");
    let child = pid.load(Ordering::Acquire); assert!(child > 0);
    assert_eq!(unsafe { libc::kill(child, 0) }, -1);
    assert_eq!(std::io::Error::last_os_error().raw_os_error(), Some(libc::ESRCH));
    assert_eq!(unsafe { libc::waitpid(child, std::ptr::null_mut(), libc::WNOHANG) }, -1);
    assert_eq!(std::io::Error::last_os_error().raw_os_error(), Some(libc::ECHILD));
    println!("PASS {error}: child {child} killed and reaped");
}

fn denied(result: libc::c_long, operation: &str) {
    assert_eq!(result, -1, "{operation} unexpectedly succeeded");
    assert_eq!(std::io::Error::last_os_error().raw_os_error(), Some(libc::EPERM), "{operation}");
}

fn sandbox_probe(args: &[String]) {
    let inherited: i32 = args[4].parse().unwrap();
    assert!(unsafe { libc::fcntl(inherited, libc::F_GETFD) } >= 0);
    sandbox::restrict(128 << 20).unwrap();
    let mut byte = 0u8;
    assert_eq!(unsafe { libc::read(inherited, (&mut byte as *mut u8).cast(), 1) }, -1);
    assert_eq!(std::io::Error::last_os_error().raw_os_error(), Some(libc::EBADF));
    for (name, flags) in [(&args[2], libc::O_RDONLY), (&args[3], libc::O_WRONLY | libc::O_CREAT)] {
        let name = std::ffi::CString::new(name.as_str()).unwrap();
        denied(unsafe { libc::open(name.as_ptr(), flags, 0o600) }.into(), "filesystem open/write");
    }
    denied(unsafe { libc::socket(libc::AF_INET, libc::SOCK_STREAM, 0) }.into(), "network socket");
    let executable = c"/bin/true";
    let argv = [executable.as_ptr(), std::ptr::null()]; let env = [std::ptr::null()];
    denied(unsafe { libc::execve(executable.as_ptr(), argv.as_ptr(), env.as_ptr()) }.into(), "exec");
    for operation in ["fork", "clone"] {
        let result = if operation == "fork" { unsafe { libc::fork() }.into() }
            else { unsafe { libc::syscall(libc::SYS_clone, libc::SIGCHLD, 0, 0, 0, 0) } };
        if result == 0 { unsafe { libc::_exit(99) }; }
        if result > 0 { unsafe { libc::waitpid(result as i32, std::ptr::null_mut(), 0) }; }
        denied(result, operation);
    }
    denied(unsafe { libc::ptrace(libc::PTRACE_TRACEME, 0, std::ptr::null_mut::<libc::c_void>(),
        std::ptr::null_mut::<libc::c_void>()) }, "ptrace");
    println!("PASS sandbox: filesystem, socket, exec, fork, clone, ptrace denied; inherited FD closed");
}

fn confinement(scratch: &Path) {
    let existing = scratch.join("sentinel"); fs::write(&existing, b"private sentinel").unwrap();
    let file = File::open(&existing).unwrap(); let fd = file.as_raw_fd();
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) }; assert!(flags >= 0);
    assert_eq!(unsafe { libc::fcntl(fd, libc::F_SETFD, flags & !libc::FD_CLOEXEC) }, 0);
    let child = Command::new(std::env::current_exe().unwrap()).arg("--sandbox-probe")
        .arg(&existing).arg(scratch.join("forbidden-write")).arg(fd.to_string())
        .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn();
    assert_eq!(unsafe { libc::fcntl(fd, libc::F_SETFD, flags) }, 0);
    let output = child.unwrap().wait_with_output().unwrap();
    assert!(output.status.success(), "sandbox probe: {}", String::from_utf8_lossy(&output.stderr));
    assert!(!scratch.join("forbidden-write").exists());
    print!("{}", String::from_utf8(output.stdout).unwrap());
}

fn children_file() -> String {
    format!("/proc/self/task/{}/children", unsafe { libc::syscall(libc::SYS_gettid) })
}

struct Family { wrapper: Child, children: String, subreaper: libc::c_int }
impl Drop for Family {
    fn drop(&mut self) {
        let _ = self.wrapper.kill(); let _ = self.wrapper.wait();
        for pid in fs::read_to_string(&self.children).unwrap_or_default().split_whitespace() {
            let pid = pid.parse::<libc::pid_t>().unwrap();
            unsafe { libc::kill(pid, libc::SIGKILL); }
            loop {
                if unsafe { libc::waitpid(pid, std::ptr::null_mut(), 0) } >= 0
                    || std::io::Error::last_os_error().raw_os_error() != Some(libc::EINTR) { break; }
            }
        }
        assert_eq!(unsafe { libc::prctl(libc::PR_SET_CHILD_SUBREAPER, self.subreaper, 0, 0, 0) }, 0);
    }
}

fn ready_line(pipe: &mut ChildStdout) -> String {
    let deadline = Instant::now() + Duration::from_secs(5); let mut line = Vec::new();
    loop {
        assert!(Instant::now() < deadline, "parent-death readiness timed out");
        let mut fd = libc::pollfd { fd: pipe.as_raw_fd(), events: libc::POLLIN, revents: 0 };
        if unsafe { libc::poll(&mut fd, 1, 20) } <= 0 { continue; }
        let mut byte = [0]; pipe.read_exact(&mut byte).unwrap();
        if byte[0] == b'\n' { return String::from_utf8(line).unwrap(); }
        line.push(byte[0]); assert!(line.len() <= 64);
    }
}

fn parent_death() {
    let rejected = Command::new(HELPER).args(["1024", "1048576", "64", "134217728", "0"])
        .stdin(Stdio::null()).output().unwrap();
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("Photo decoder parent changed"));
    let children = children_file(); assert!(fs::read_to_string(&children).unwrap().trim().is_empty());
    let mut subreaper: libc::c_int = 0;
    assert_eq!(unsafe { libc::prctl(libc::PR_GET_CHILD_SUBREAPER, &mut subreaper, 0, 0, 0) }, 0);
    assert_eq!(unsafe { libc::prctl(libc::PR_SET_CHILD_SUBREAPER, 1, 0, 0, 0) }, 0);
    let wrapper = Command::new(std::env::current_exe().unwrap()).arg("--parent-wrapper")
        .stdin(Stdio::null()).stdout(Stdio::piped()).spawn().unwrap();
    let mut family = Family { wrapper, children, subreaper };
    let mut pipe = family.wrapper.stdout.take().unwrap(); let mut pid = None; let mut ready = false;
    for _ in 0..2 {
        let line = ready_line(&mut pipe);
        if line == "ready" { ready = true; }
        else { pid = Some(line.strip_prefix("child ").unwrap().parse::<libc::pid_t>().unwrap()); }
    }
    assert!(ready); let pid = pid.unwrap();
    family.wrapper.kill().unwrap(); family.wrapper.wait().unwrap();
    let deadline = Instant::now() + Duration::from_secs(5); let mut status = 0;
    loop {
        let waited = unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) };
        if waited == pid { break; }
        assert_eq!(waited, 0); assert!(Instant::now() < deadline, "orphan decoder survived parent death");
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(libc::WIFSIGNALED(status)); assert_eq!(libc::WTERMSIG(status), libc::SIGKILL);
    assert_eq!(unsafe { libc::waitpid(pid, std::ptr::null_mut(), libc::WNOHANG) }, -1);
    assert_eq!(std::io::Error::last_os_error().raw_os_error(), Some(libc::ECHILD));
    println!("PASS wrong-parent rejection and parent-death SIGKILL; adopted child reaped");
}

fn parent_wrapper() {
    let mut child = Command::new(std::env::current_exe().unwrap()).arg("--parent-child")
        .arg(std::process::id().to_string()).spawn().unwrap();
    println!("child {}", child.id()); std::io::stdout().flush().unwrap();
    child.wait().unwrap();
}

fn parent_child(parent: usize) {
    sandbox::parent_death(parent).unwrap(); sandbox::restrict(128 << 20).unwrap();
    println!("ready"); std::io::stdout().flush().unwrap();
    loop { std::thread::sleep(Duration::from_secs(60)); }
}

fn measure(path: &Path, isolated: bool) {
    use std::hash::{Hash, Hasher};
    let start = Instant::now();
    let photo = if isolated {
        photo_reader::decode(File::open(path).unwrap(), limits(), &AtomicBool::new(false), Path::new(HELPER), Duration::from_secs(120)).unwrap()
    } else {
        layer_color::photo::read_photo_detailed(std::io::BufReader::new(File::open(path).unwrap()), limits()).unwrap()
    };
    let elapsed = start.elapsed();
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    let mut rows = photo.source.rows(); let mut row = vec![0; photo.source.row_bytes()];
    for y in 0..photo.source.extent[1] { rows.read(y, &mut row).unwrap(); row.hash(&mut hash); }
    println!("isolated={isolated} extent={:?} decode_wall={elapsed:?} rows_hash={:016x} metadata={:?}",
        photo.source.extent, hash.finish(), photo.metadata);
    for (name, who) in [("parent", libc::RUSAGE_SELF), ("child", libc::RUSAGE_CHILDREN)] {
        let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
        assert_eq!(unsafe { libc::getrusage(who, usage.as_mut_ptr()) }, 0);
        println!("{name} peak_rss_kib={}", unsafe { usage.assume_init() }.ru_maxrss);
    }
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).is_some_and(|arg| arg == "--sandbox-probe") { sandbox_probe(&args); return; }
    if args.get(1).is_some_and(|arg| arg == "--parent-wrapper") { parent_wrapper(); return; }
    if args.get(1).is_some_and(|arg| arg == "--parent-child") { parent_child(args[2].parse().unwrap()); return; }
    if args.len() == 6 && args[1..].iter().all(|arg| arg.parse::<usize>().is_ok()) {
        fake_helper(args[1].parse().unwrap()); return;
    }
    let scratch = Scratch::new();
    if args.get(1).is_some_and(|arg| arg == "--measure-direct" || arg == "--measure-isolated") {
        measure(Path::new(args.get(2).expect("JPEG fixture path")), args[1] == "--measure-isolated"); return;
    }
    confinement(&scratch.0);
    parent_death();
    formats(); failures(&scratch.0); interruption(true); interruption(false);
    println!("PASS photo-reader integration");
}
