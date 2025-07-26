use std::ffi::CStr;
use std::os::raw::c_char;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::thread;
use crossbeam_channel::{bounded, Sender};
use image::{ImageBuffer, Rgba};
use std::fs::create_dir_all;
use std::process::Command;
use std::process::Stdio;

// === Frame Struct ===
struct FrameData {
    pixels: Vec<u8>,
    width: u32,
    height: u32,
    index: u32,
}

// === Recording Session ===
struct RecordingSession {
    sender: Sender<FrameData>,
    handle: thread::JoinHandle<()>,
    frame_counter: u32,
}

fn log(msg: &str) {
    use std::fs::OpenOptions;
    use std::io::Write;

    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open("/Users/rafajm/Library/Application Support/Godot/app_userdata/PP_auto/sniplog.txt")
    {
        let _ = writeln!(file, "{}", msg);
    }
}


impl RecordingSession {
    fn new(output_dir: PathBuf) -> std::io::Result<Self> {
        create_dir_all(&output_dir)?;

        let (sender, receiver) = bounded::<FrameData>(1000);

        let handle = thread::spawn(move || {
            for frame in receiver {
                let filename = format!("frame_{:05}.png", frame.index);
                let filepath = output_dir.join(filename);
                if let Some(img) = ImageBuffer::<Rgba<u8>, _>::from_raw(frame.width, frame.height, frame.pixels) {
                    let _ = img.save(filepath);
                    log(&format!("Saving frame {}", frame.index));
                }
            }
        });

        Ok(Self {
            sender,
            handle,
            frame_counter: 0,
        })
    }

    fn queue_frame(&mut self, pixels: &[u8], width: u32, height: u32) -> bool {
        let frame = FrameData {
            pixels: pixels.to_vec(),
            width,
            height,
            index: self.frame_counter,
        };
        self.frame_counter += 1;
        self.sender.send(frame).is_ok()
    }

    fn stop(self) {
        // Dropping sender causes the thread to exit (receiver gets EOF)
        drop(self.sender);
        let _ = self.handle.join();
    }
}

// === Global Session ===
static SESSION: OnceLock<Mutex<Option<RecordingSession>>> = OnceLock::new();

fn session() -> &'static Mutex<Option<RecordingSession>> {
    SESSION.get_or_init(|| Mutex::new(None))
}

#[no_mangle]
pub extern "C" fn start_recording(output_path: *const c_char) -> bool {
    let c_str = unsafe { CStr::from_ptr(output_path) };
    let path_str = match c_str.to_str() {
        Ok(s) => s,
        Err(_) => return false,
    };

    let path = PathBuf::from(path_str);
    match RecordingSession::new(path) {
        Ok(sess) => {
            let mut guard = session().lock().unwrap();
            *guard = Some(sess);
            true
        }
        Err(_) => false,
    }
}

#[no_mangle]
pub extern "C" fn save_frame(pixels_ptr: *const u8, width: u32, height: u32) -> bool {
    if pixels_ptr.is_null() {
        return false;
    }

    let num_bytes = (width * height * 4) as usize;
    let pixels = unsafe { std::slice::from_raw_parts(pixels_ptr, num_bytes) };

    let mut guard = session().lock().unwrap();
    if let Some(ref mut s) = *guard {
        s.queue_frame(pixels, width, height)
    } else {
        false
    }
}

#[no_mangle]
pub extern "C" fn stop_recording() -> bool {
    let mut guard = session().lock().unwrap();
    if let Some(session) = guard.take() {
        session.stop();
        true
    } else {
        false
    }
}

#[no_mangle]
pub extern "C" fn make_video(input_dir: *const c_char, output_path: *const c_char) -> bool {
    
    let input_cstr = unsafe {
        if input_dir.is_null() {
            return false;
        }
        CStr::from_ptr(input_dir)
    };
    let input_str = match input_cstr.to_str() {
        Ok(s) => s,
        Err(_) => return false,
    };

    let output_cstr = unsafe {
        if output_path.is_null() {
            return false;
        }
        CStr::from_ptr(output_path)
    };
    let output_str = match output_cstr.to_str() {
        Ok(s) => s,
        Err(_) => return false,
    };

    let input_pattern = format!("{}/frame_%05d.png", input_str);

    let result = Command::new("/opt/homebrew/bin/ffmpeg")
        .args([
            "-y",
            "-framerate", "10",
            "-i", &input_pattern,
            "-c:v", "libx264",
            "-pix_fmt", "yuv420p",
            output_str,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();

    match result {
        Ok(output) => {
            log(&format!("🔧 FFmpeg stdout:\n{}", String::from_utf8_lossy(&output.stdout)));
            log(&format!("❗️FFmpeg stderr:\n{}", String::from_utf8_lossy(&output.stderr)));
            output.status.success()
        }
        Err(e) => {
            log(&format!("❌ Failed to run FFmpeg: {}", e));
            false
        }
    }


    // result.map(|s| s.success()).unwrap_or_else(|error| {
    //     log(&format!("Problem making video: {}", error));
    //     false // or any default value expected by the context
    // })

}
