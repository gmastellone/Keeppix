#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::process::Command;

use keeppix_media::video;

#[test]
fn poster_extracts_one_frame() {
    if !video::ffprobe_available() {
        eprintln!("skipping: ffprobe not in PATH");
        return;
    }
    let dir = std::env::temp_dir().join(format!("kpx-vid-{}", uuid_like()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("clip.mp4");
    let dest = dir.join("poster.jpg");
    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=c=red:s=64x64:d=1",
            "-t",
            "1",
            "-pix_fmt",
            "yuv420p",
        ])
        .arg(&src)
        .status()
        .expect("spawn ffmpeg");
    if !status.success() {
        eprintln!("skipping: ffmpeg could not write a clip");
        let _ = std::fs::remove_dir_all(&dir);
        return;
    }
    let info = video::probe(&src).expect("probe");
    assert!(info.duration >= std::time::Duration::from_millis(100));
    video::extract_poster(&src, &dest).expect("poster");
    assert!(dest.is_file());
    let _ = std::fs::remove_dir_all(&dir);
}

fn uuid_like() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}
