use std::ffi::CStr;
use std::fs::create_dir_all;
use std::os::raw::c_char;
use std::path::PathBuf;
use std::sync::Mutex;
use once_cell::sync::Lazy;
use std::thread;
use std::sync::atomic::{AtomicBool, Ordering};
use crossbeam_channel::{Sender, Receiver, bounded};
use image::{ImageBuffer, Rgba};
use std::process::Command;

// === Threaded Frame Worker ===
struct RecordingSession {
    output_dir: PathBuf,
    sender: Sender<FrameData>,
    running: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
    frame_counter: Arc<Mutex<u32>>,
}

struct FrameData {
    pixels: Vec<u8>,
    width: u32,
    height: u32,
    frame_index: u32,
}

use std::sync::Arc;

impl RecordingSession {
    fn new(output_dir: PathBuf) -> std::io::Result<Self> {
        if !output_dir.exists() {
            create_dir_all(&output_dir)?;
        }

        let (sender, receiver): (Sender<FrameData>, Receiver<FrameData>) = bounded(60);
        let running = Arc::new(AtomicBool::new(true));
        let frame_counter = Arc::new(Mutex::new(0));

        let dir_clone = output_dir.clone();
        let running_clone = running.clone();
        let counter_clone = frame_counter.clone();

        let handle = thread::spawn(move || {
            while running_clone.load(Ordering::Relaxed) {
                if let Ok(frame) = receiver.recv() {
                    let filename = format!("frame_{:05}.png", frame.frame_index);
                    let filepath = dir_clone.join(filename);

                    if let Some(image) = ImageBuffer::<Rgba<u8>, _>::from_raw(frame.width, frame.height, frame.pixels) {
                        let _ = image.save(filepath);
                    }
                }
            }
        });

        Ok(Self {
            output_dir,
            sender,
            running,
            handle: Some(handle),
            frame_counter,
        })
    }

    fn queue_frame(&self, pixels: &[u8], width: u32, height: u32) -> bool {
        let mut counter = self.frame_counter.lock().unwrap();
        let index = *counter;
        *counter += 1;

        let data = FrameData {
            pixels: pixels.to_vec(),
            width,
            height,
            frame_index: index,
        };

        self.sender.try_send(data).is_ok()
    }

    fn stop(self) {
        self.running.store(false, Ordering::Relaxed);
        drop(self.sender);
        if let Some(handle) = self.handle {
            let _ = handle.join();
        }
    }
}

static SESSION: Lazy<Mutex<Option<RecordingSession>>> = Lazy::new(|| Mutex::new(None));

#[no_mangle]
pub extern "C" fn start_recording(output_path: *const c_char) -> bool {
    let c_str = unsafe {
        if output_path.is_null() {
            return false;
        }
        CStr::from_ptr(output_path)
    };

    let path_str = match c_str.to_str() {
        Ok(s) => s,
        Err(_) => return false,
    };

    let path = PathBuf::from(path_str);

    match RecordingSession::new(path) {
        Ok(session) => {
            let mut global = SESSION.lock().unwrap();
            *global = Some(session);
            true
        },
        Err(_) => false,
    }
}

#[no_mangle]
pub extern "C" fn save_frame(pixels_ptr: *const u8, width: u32, height: u32) -> bool {
    if pixels_ptr.is_null() {
        return false;
    }

    let total_bytes = (width * height * 4) as usize;
    let buffer = unsafe { std::slice::from_raw_parts(pixels_ptr, total_bytes) };

    let global = SESSION.lock().unwrap();
    if let Some(ref session) = *global {
        session.queue_frame(buffer, width, height)
    } else {
        false
    }
}

#[no_mangle]
pub extern "C" fn stop_recording() -> bool {
    let mut global = SESSION.lock().unwrap();
    if let Some(session) = global.take() {
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

    let result = Command::new("ffmpeg")
        .args([
            "-y",
            "-framerate", "10",
            "-i", &input_pattern,
            "-c:v", "libx264",
            "-pix_fmt", "yuv420p",
            output_str,
        ])
        .status();

    result.map(|s| s.success()).unwrap_or(false)
}
