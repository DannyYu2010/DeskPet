//! DeskPet's asset pipeline: photo in, cutout out.
//!
//! A separate binary on purpose. The app must carry no ML dependency at rest
//! (HANDOFF.md's resource budget), and "we load the model and free it again" is
//! a claim about allocator behaviour that nobody can check from outside. A
//! process that exits releases everything, provably. It also means a crash
//! inside inference cannot take the pet down with it.
//!
//!     deskpet-assetpipe cutout --input photo.jpg --output cut.png \
//!                              --model-dir ~/Library/Application\ Support/…/models
//!
//! stdout carries the result path and nothing else, so the caller can read it
//! without parsing prose. Progress and diagnostics go to stderr, prefixed
//! `progress:` where they are meant for a human waiting.

mod model;

use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use image::{imageops::FilterType, RgbaImage};

fn main() {
    if let Err(e) = run() {
        // `{:#}` so the whole anyhow chain reaches the caller; the app shows
        // this text to the user verbatim.
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

struct Args {
    /// Either a single file, or a directory of PNGs to process as a batch.
    input: PathBuf,
    output: PathBuf,
    model_dir: PathBuf,
}

fn parse_args() -> Result<Args> {
    let mut args = std::env::args().skip(1);
    let cmd = args.next().unwrap_or_default();
    if cmd != "cutout" {
        bail!("usage: deskpet-assetpipe cutout --input <img> --output <png> --model-dir <dir>");
    }

    let (mut input, mut output, mut model_dir) = (None, None, None);
    while let Some(flag) = args.next() {
        let mut value = || {
            args.next()
                .map(PathBuf::from)
                .with_context(|| format!("{flag} needs a value"))
        };
        match flag.as_str() {
            "--input" => input = Some(value()?),
            "--output" => output = Some(value()?),
            "--model-dir" => model_dir = Some(value()?),
            other => bail!("unknown flag {other}"),
        }
    }

    Ok(Args {
        input: input.context("--input is required")?,
        output: output.context("--output is required")?,
        model_dir: model_dir.context("--model-dir is required")?,
    })
}

fn run() -> Result<()> {
    let args = parse_args()?;
    let model_path = model::ensure(&args.model_dir)?;

    // One session for the whole batch. A clip is dozens of frames, and loading
    // a 224 MB model per frame would dominate the runtime completely — this is
    // the difference between an import that takes seconds and one that takes
    // minutes.
    let mut session = open_session(&model_path)?;

    let jobs = collect_jobs(&args)?;
    if jobs.is_empty() {
        bail!("nothing to do: {} matched no images", args.input.display());
    }

    for (i, (src, dst)) in jobs.iter().enumerate() {
        eprintln!("progress: cutting out {} of {}", i + 1, jobs.len());

        let source = image::open(src)
            .with_context(|| format!("opening {}", src.display()))?
            .to_rgba8();
        let (w, h) = source.dimensions();
        if w == 0 || h == 0 {
            bail!("{} has no pixels", src.display());
        }

        let alpha = infer(&mut session, &source)?;

        let mut out = source;
        for (i, px) in out.pixels_mut().enumerate() {
            px.0[3] = alpha[i];
        }

        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        out.save(dst)
            .with_context(|| format!("writing {}", dst.display()))?;
        println!("{}", dst.display());
    }

    Ok(())
}

/// Pair every input with where its cutout goes.
///
/// A directory in means a directory out, one file per input, keeping names so
/// the caller can match frames back to their order without a manifest.
fn collect_jobs(args: &Args) -> Result<Vec<(PathBuf, PathBuf)>> {
    if args.input.is_file() {
        return Ok(vec![(args.input.clone(), args.output.clone())]);
    }
    if !args.input.is_dir() {
        bail!("{} is neither a file nor a directory", args.input.display());
    }

    let mut names: Vec<PathBuf> = std::fs::read_dir(&args.input)
        .with_context(|| format!("reading {}", args.input.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.is_file()
                && matches!(
                    p.extension()
                        .and_then(|e| e.to_str())
                        .map(str::to_ascii_lowercase)
                        .as_deref(),
                    Some("png" | "jpg" | "jpeg" | "webp")
                )
        })
        .collect();

    // Frame order matters and readdir order does not.
    names.sort();

    Ok(names
        .into_iter()
        .map(|src| {
            let name = src.file_name().unwrap_or_default().to_owned();
            let dst = args.output.join(name).with_extension("png");
            (src, dst)
        })
        .collect())
}

fn open_session(model_path: &std::path::Path) -> Result<ort::session::Session> {
    ort::session::Session::builder()
        .context("creating an ONNX session builder")?
        .commit_from_file(model_path)
        .with_context(|| format!("loading {}", model_path.display()))
}

/// Run the model and return a full-resolution alpha channel.
fn infer(session: &mut ort::session::Session, source: &RgbaImage) -> Result<Vec<u8>> {
    use ort::value::Tensor;

    let (w, h) = source.dimensions();
    let n = model::INPUT_SIZE;

    // Bilinear, matching the model card's preprocessor. Using a sharper filter
    // here would not be "better": the network saw bilinear during training and
    // the mismatch shows up as speckle along edges.
    let small = image::imageops::resize(source, n, n, FilterType::Triangle);

    // NCHW, ImageNet-normalised.
    let mut input = vec![0f32; (3 * n * n) as usize];
    let plane = (n * n) as usize;
    for y in 0..n {
        for x in 0..n {
            let px = small.get_pixel(x, y).0;
            let i = (y * n + x) as usize;
            for c in 0..3 {
                input[c * plane + i] = (px[c] as f32 / 255.0 - model::MEAN[c]) / model::STD[c];
            }
        }
    }

    let tensor = Tensor::from_array((vec![1_i64, 3, n as i64, n as i64], input))
        .context("building the input tensor")?;
    let outputs = session
        .run(ort::inputs![tensor])
        .context("running the matte model")?;

    // BiRefNet exports sometimes carry several outputs; the mask is the one the
    // model card names. Fall back to the first rather than guessing by shape.
    // Bound to a local first: `outputs.values().next()` yields an owned
    // `Option<ValueRef>`, and dereferencing it inline would borrow a temporary
    // that dies at the end of the statement.
    let first = outputs.values().next();
    let value = outputs
        .get("output_image")
        .or(first.as_deref())
        .context("the model produced no output")?;
    let (shape, logits) = value
        .try_extract_tensor::<f32>()
        .context("the model output was not a float tensor")?;

    let count = logits.len();
    if count < (n * n) as usize {
        bail!("mask is {count} values for a {n}x{n} input (shape {shape:?})");
    }

    // Logits, not probabilities — the model card applies sigmoid afterwards.
    let mut mask = image::GrayImage::new(n, n);
    for (i, px) in mask.pixels_mut().enumerate() {
        let p = 1.0 / (1.0 + (-logits[i]).exp());
        px.0[0] = (p * 255.0).clamp(0.0, 255.0) as u8;
    }

    // Back to the source resolution. The mask is smooth, so a good filter here
    // is worth it — this edge is what the user actually looks at.
    let full = image::imageops::resize(&mask, w, h, FilterType::CatmullRom);
    Ok(full.into_raw())
}
