#![allow(clippy::unwrap_used, clippy::expect_used)]

mod harness;

use std::process::Command;

use chrono::{TimeZone, Utc};
use harness::TestServer;
use keeppix_db::{AssetRepo, FolderRepo, LibraryRepo, UserRepo};
use keeppix_domain::{
    AssetKind, AssetName, AuthContext, FolderId, NewAsset, NewLibrary, SystemRole, Username,
};
use keeppix_media::transcode::{TranscodeProfile, transcode_cache_dir};
use serde_json::json;

#[tokio::test]
async fn h264_mp4_playback_is_direct_with_video_mp4_accept() {
    let server = TestServer::start().await;
    let (folder, root) = seed_library(&server).await;
    let asset = seed_video_asset(&server, folder, &root, "clip.mp4").await;
    write_h264_clip(&root.join("clip.mp4"));

    let response = server
        .client
        .get(server.url(&format!(
            "/media/video/{}/playback?save_bandwidth=false",
            asset.id
        )))
        .header("accept", "video/mp4")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["mode"], "direct");
    assert!(
        body["url"]
            .as_str()
            .unwrap()
            .contains(&format!("/media/original/{}", asset.id))
    );
    assert!(body["ready"].as_bool().unwrap());
}

#[tokio::test]
async fn mkv_playback_requests_hls_transcode() {
    let server = TestServer::start().await;
    let (folder, root) = seed_library(&server).await;
    let asset = seed_video_asset(&server, folder, &root, "clip.mkv").await;
    write_mkv_clip(&root.join("clip.mkv"));

    let response = server
        .client
        .get(server.url(&format!(
            "/media/video/{}/playback?save_bandwidth=false",
            asset.id
        )))
        .header("accept", "video/mp4")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["mode"], "hls");
    assert!(!body["ready"].as_bool().unwrap());
}

#[tokio::test]
async fn missing_hls_segment_is_not_found() {
    let server = TestServer::start().await;
    let (folder, root) = seed_library(&server).await;
    let asset = seed_video_asset(&server, folder, &root, "clip.mp4").await;
    write_h264_clip(&root.join("clip.mp4"));

    let response = server
        .client
        .get(server.url(&format!(
            "/media/video/{}/hls/seg999.ts?save_bandwidth=false",
            asset.id
        )))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 404);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["type"], "keeppix/not-found");
}

#[tokio::test]
async fn cached_hls_playlist_is_served() {
    let server = TestServer::start().await;
    let (folder, root) = seed_library(&server).await;
    let asset = seed_video_asset(&server, folder, &root, "clip.mp4").await;
    write_h264_clip(&root.join("clip.mp4"));
    let hash = [0x11_u8; 32];
    AssetRepo::new(&server.db)
        .set_hash(asset.id, hash)
        .await
        .unwrap();
    let cache = transcode_cache_dir(&server.data_dir, &hash, TranscodeProfile::SaveBandwidth720p);
    std::fs::create_dir_all(&cache).unwrap();
    std::fs::write(
        cache.join("index.m3u8"),
        "#EXTM3U\n#EXTINF:4.0,\nseg000.ts\n",
    )
    .unwrap();
    std::fs::write(cache.join("seg000.ts"), b"fake-segment").unwrap();

    let response = server
        .client
        .get(server.url(&format!(
            "/media/video/{}/hls/index.m3u8?save_bandwidth=false",
            asset.id
        )))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "application/vnd.apple.mpegurl"
    );
}

async fn setup(server: &TestServer) {
    server
        .client
        .post(server.url("/api/v1/setup"))
        .json(&json!({
            "username": "giovanni",
            "display_name": "Giovanni",
            "password": "correct horse battery staple"
        }))
        .send()
        .await
        .unwrap();
}

async fn seed_library(server: &TestServer) -> (FolderId, std::path::PathBuf) {
    setup(server).await;
    let username = Username::parse("giovanni").unwrap();
    let (user, _) = UserRepo::new(&server.db)
        .find_by_username(&username)
        .await
        .unwrap()
        .expect("admin");
    let ctx = AuthContext::user(user.id, SystemRole::Admin);
    let root = server.data_dir.join("library");
    std::fs::create_dir_all(&root).unwrap();
    let library = LibraryRepo::new(&server.db)
        .create(
            &ctx,
            NewLibrary {
                name: "Foto".to_owned(),
                owner_id: user.id,
                root_path: root.clone(),
                exclude_patterns: vec![],
            },
        )
        .await
        .unwrap();
    let folder = FolderRepo::new(&server.db)
        .ensure_path(library.id, &[])
        .await
        .unwrap();
    (folder.id, root)
}

async fn seed_video_asset(
    server: &TestServer,
    folder: FolderId,
    root: &std::path::Path,
    name: &str,
) -> keeppix_domain::Asset {
    let asset = AssetRepo::new(&server.db)
        .upsert_discovered(NewAsset {
            folder_id: folder,
            filename: AssetName::parse(name).unwrap(),
            size_bytes: 1,
            mtime: Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(),
            inode: Some(1),
            kind: AssetKind::Video,
        })
        .await
        .unwrap()
        .unwrap();
    let hash = [0x22_u8; 32];
    AssetRepo::new(&server.db)
        .set_hash(asset.id, hash)
        .await
        .unwrap();
    let _ = root;
    asset
}

fn write_mkv_clip(path: &std::path::Path) {
    if path.is_file() {
        return;
    }
    std::fs::create_dir_all(path.parent().unwrap()).ok();
    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=c=green:s=320x240:d=1",
            "-t",
            "1",
            "-pix_fmt",
            "yuv420p",
            "-f",
            "matroska",
        ])
        .arg(path)
        .status();
    if status.as_ref().is_ok_and(std::process::ExitStatus::success) {
        return;
    }
    std::fs::write(path, b"not-a-real-video").unwrap();
}

fn write_h264_clip(path: &std::path::Path) {
    if path.is_file() {
        return;
    }
    std::fs::create_dir_all(path.parent().unwrap()).ok();
    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=c=blue:s=320x240:d=1",
            "-t",
            "1",
            "-pix_fmt",
            "yuv420p",
        ])
        .arg(path)
        .status();
    if status.as_ref().is_ok_and(std::process::ExitStatus::success) {
        return;
    }
    std::fs::write(path, b"not-a-real-video-but-enough-for-auth-tests").unwrap();
}
