use std::process::Command;

const QUALITY_LEVELS: &[u8] = &[20, 25, 30, 35, 40];

fn main() -> color_eyre::Result<()> {
    let cli = <Cli as clap::Parser>::parse();

    if !cli.path.is_file() {
        eyre::bail!("{} is not a file", cli.path.display());
    }

    if !ffmpeg_is_available() {
        eyre::bail!("ffmpeg is not available");
    }

    let path = cli.path.canonicalize()?;
    let basename = path
        .file_stem()
        .ok_or_else(|| eyre::eyre!("no file name found in {}", cli.path.display()))?
        .to_str()
        .ok_or_else(|| eyre::eyre!("{} is an invalid utf-8 path", cli.path.display()))?;
    let parent = path
        .parent()
        .ok_or_else(|| eyre::eyre!("{} has no parent directory", cli.path.display()))?;

    let mut files = Vec::new();
    for level in QUALITY_LEVELS {
        let filename = format!("{}_hevc_q{}.mp4", basename, level);
        let encoded_path = parent.join(filename);

        if std::fs::exists(&encoded_path)? {
            eprintln!("{} already exists, skipping", encoded_path.display());
            files.push(encoded_path);
            continue;
        }

        let status = Command::new("ffmpeg")
            .args([
                "-i",
                path.to_str().unwrap(),
                "-c:v",
                "hevc_videotoolbox",
                "-q:v",
                &*level.to_string(),
                "-c:a",
                "copy",
                encoded_path.to_str().unwrap(),
            ])
            .status()?;

        if !status.success() {
            eyre::bail!("ffmpeg returned {}", status.code().unwrap());
        }

        files.push(encoded_path);
    }

    let original_size = std::fs::metadata(&path)?.len();

    let mut results = vec![VmafResult {
        filename: basename.into(),
        vmaf: 100.,
        size_bytes: original_size,
        compression_ratio: 1.0,
    }];
    let re = regex::Regex::new(r".*VMAF score: ([0-9.]+).*").unwrap();

    for file in files {
        let encoded_size = std::fs::metadata(&file)?.len();

        let vmaf_output = Command::new("ffmpeg")
            .args([
                "-i",
                file.to_str().unwrap(),
                "-i",
                path.to_str().unwrap(),
                "-filter_complex",
                "libvmaf",
                "-f",
                "null",
                "-",
            ])
            .output()?;
        let vmaf_err = String::from_utf8_lossy(&vmaf_output.stderr);
        let vmaf_output = String::from_utf8_lossy(&vmaf_output.stdout);

        let Some(score) = re
            .captures(&vmaf_err)
            .map(|caps| caps.extract())
            .and_then(|(_, [score])| score.parse::<f64>().ok())
        else {
            eprintln!("unable to determine VMAF score for {}", file.display());
            eprintln!("-----ffmpeg stdout----");
            eprintln!("{}", vmaf_output);
            eprintln!("-----ffmpeg stderr----");
            eprintln!("{}", vmaf_err);
            continue;
        };

        results.push(VmafResult {
            filename: file.file_stem().unwrap().to_str().unwrap().into(),
            vmaf: score,
            size_bytes: encoded_size,
            compression_ratio: (original_size as f64) / (encoded_size as f64),
        })
    }

    let mut writer = csv::Writer::from_path(&cli.output)?;

    for result in results {
        writer.serialize(result)?;
    }

    writer.flush().unwrap();
    println!("results written to {}", cli.output);

    Ok(())
}

fn ffmpeg_is_available() -> bool {
    Command::new("ffmpeg").arg("--help").output().is_ok()
}

#[derive(Debug, clap::Parser)]
struct Cli {
    #[clap(short, long, default_value_t=String::from("./results.csv"))]
    output: String,
    path: std::path::PathBuf,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct VmafResult {
    filename: String,
    vmaf: f64,
    size_bytes: u64,
    compression_ratio: f64,
}
