//! Fetching and verifying the matte model.
//!
//! The weights are never committed (HANDOFF.md) — 224 MB of binary in git is
//! a repository nobody wants to clone. They are downloaded once, verified by
//! hash, and reused.
//!
//! The revision is pinned rather than tracking `main`: a Hugging Face branch is
//! mutable, so `main` today and `main` next month are not the same file, and a
//! model that silently changes under a product is a support nightmare.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};

pub const REPO: &str = "onnx-community/BiRefNet_lite-ONNX";
pub const REVISION: &str = "de15b22ba131738a16dff04aab8bdf8dc32e3ac1";
pub const FILE: &str = "onnx/model.onnx";
pub const SHA256: &str = "5600024376f572a557870a5eb0afb1e5961636bef4e1e22132025467d0f03333";
pub const BYTES: u64 = 224_005_088;

/// Model input edge, from the repo's preprocessor_config.json.
pub const INPUT_SIZE: u32 = 1024;
/// ImageNet normalisation, same source.
pub const MEAN: [f32; 3] = [0.485, 0.456, 0.406];
pub const STD: [f32; 3] = [0.229, 0.224, 0.225];

fn url() -> String {
    format!("https://huggingface.co/{REPO}/resolve/{REVISION}/{FILE}")
}

pub fn path_in(dir: &Path) -> PathBuf {
    dir.join(format!("birefnet_lite-{}.onnx", &REVISION[..12]))
}

/// Return the model path, downloading it if it is not already there.
///
/// Verification is not optional and not a warning. This file is executed as a
/// program by the ONNX runtime; a truncated download and a tampered one are
/// indistinguishable without the hash, and both deserve the same refusal.
pub fn ensure(dir: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let dest = path_in(dir);

    if dest.is_file() {
        match verify(&dest) {
            Ok(()) => return Ok(dest),
            Err(e) => {
                eprintln!("cached model rejected ({e}); downloading again");
                std::fs::remove_file(&dest).ok();
            }
        }
    }

    // Download beside the target and rename on success, so an interrupted run
    // never leaves something that looks like a usable model.
    let tmp = dest.with_extension("part");
    download(&url(), &tmp)?;
    verify(&tmp).context("the downloaded model failed its hash check")?;
    std::fs::rename(&tmp, &dest)?;
    Ok(dest)
}

fn download(url: &str, dest: &Path) -> Result<()> {
    eprintln!(
        "progress: downloading the matte model, {} MB",
        BYTES / 1_048_576
    );

    let resp = ureq::get(url)
        .call()
        .with_context(|| format!("requesting {url}"))?;
    let mut reader = resp.into_reader();
    let mut file =
        std::fs::File::create(dest).with_context(|| format!("creating {}", dest.display()))?;

    let mut buf = vec![0u8; 1 << 20];
    let mut done: u64 = 0;
    let mut last_report = 0u64;

    loop {
        let n = reader.read(&mut buf).context("reading the model stream")?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])?;
        done += n as u64;

        // Report every few percent. This is a multi-minute wait on a slow
        // connection and silence during it reads as a hang.
        if done - last_report > BYTES / 25 {
            last_report = done;
            eprintln!("progress: {}%", (done * 100 / BYTES).min(99));
        }
    }

    file.flush()?;
    Ok(())
}

fn verify(path: &Path) -> Result<()> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let got = format!("{:x}", hasher.finalize());
    if got != SHA256 {
        bail!("sha256 mismatch: expected {SHA256}, got {got}");
    }
    Ok(())
}
