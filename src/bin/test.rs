use sniptool::{start_recording, save_frame, stop_recording, make_video};
use std::ffi::CString;

fn main() {
    let dir = "/tmp/snip_frames";
    let c_path = CString::new(dir).unwrap();

    let success = unsafe { start_recording(c_path.as_ptr()) };
    if !success {
        println!("❌ Failed to start recording");
        return;
    }

    println!("✅ Started recording at {}", dir);

    let width = 256;
    let height = 256;

    // Frame 1: Red
    let red = vec![255, 0, 0, 255];
    let red_frame = red.repeat((width * height) as usize);
    unsafe {
        save_frame(red_frame.as_ptr(), width, height);
    }

    // Frame 2: Green
    let green = vec![0, 255, 0, 255];
    let green_frame = green.repeat((width * height) as usize);
    unsafe {
        save_frame(green_frame.as_ptr(), width, height);
    }

    // Frame 3: Blue
    let blue = vec![0, 0, 255, 255];
    let blue_frame = blue.repeat((width * height) as usize);
    unsafe {
        save_frame(blue_frame.as_ptr(), width, height);
    }

    unsafe {
        stop_recording();
    }

    println!("✅ Saved 3 frames as PNGs");

    let out_path = CString::new("/tmp/snip_output.mp4").unwrap();
    let success = unsafe {
        make_video(c_path.as_ptr(), out_path.as_ptr())
    };

    if success {
        println!("✅ Video created!");
    } else {
        println!("❌ Failed to create video.");
    }
}
