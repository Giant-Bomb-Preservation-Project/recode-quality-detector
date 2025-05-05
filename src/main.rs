mod ffmpeg;

use promkit::preset::{checkbox::Checkbox, listbox::Listbox};
use std::process::Command;

const QUALITY_LEVELS: &[u8] = &[100, 99, 95, 90, 80, 60, 40, 20];

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

fn print_results(results: &[VmafResult]) -> color_eyre::Result<()> {
    use comfy_table::presets::UTF8_FULL;
    use comfy_table::*;

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_width(80)
        .set_header(vec![
            Cell::new("filename"),
            Cell::new("VMAF Score"),
            Cell::new("size (bytes)"),
            Cell::new("compression ratio"),
        ]);

    for result in results {
        table.add_row(vec![
            Cell::new(result.filename.clone()).add_attribute(Attribute::Bold),
            Cell::new(result.vmaf),
            Cell::new(result.size_bytes),
            Cell::new(result.compression_ratio),
        ]);
    }

    println!("{table}");
    Ok(())
}

fn main() -> color_eyre::Result<()> {
    let cli = <Cli as clap::Parser>::parse();

    if !cli.path.is_file() {
        eyre::bail!("{} is not a file", cli.path.display());
    }

    if !ffmpeg::is_available() {
        eyre::bail!("ffmpeg is not available");
    }

    let codecs = prompt_codecs()?;
    print_results(&encode_and_evaluate(cli.path, &codecs)?)
}

fn prompt_codecs() -> color_eyre::Result<Vec<ffmpeg::Codec>> {
    const HARDWARE: &str = "All Hardware Encoding Available Codecs";
    const RECOMMENDED_SET: &str = "H.264 / H.265 / H.266 / AV1";
    const CUSTOM: &str = "Custom";

    let answer = Listbox::new(vec![HARDWARE, RECOMMENDED_SET, CUSTOM])
        .title("Which video codecs would you like to test?")
        .prompt()
        .map_err(|error| eyre::eyre!(error))?
        .run()
        .map_err(|error| eyre::eyre!(error))?;

    let codecs = ffmpeg::codecs()?;
    Ok(match &*answer {
        HARDWARE => codecs
            .into_iter()
            .filter(|codec| matches!(codec.kind, ffmpeg::CodecKind::Video) && codec.encodable)
            .filter(|codec| codec.encoders.iter().any(|coder| is_hardware_coder(coder)))
            .collect(),
        RECOMMENDED_SET => codecs
            .into_iter()
            .filter(|codec| matches!(&*codec.extension, "h264" | "hevc" | "av1" | "vvc"))
            .collect(),
        CUSTOM => {
            let selected = Checkbox::new(
                codecs
                    .iter()
                    .filter(|codec| {
                        matches!(codec.kind, ffmpeg::CodecKind::Video) && codec.encodable
                    })
                    .map(|codec| codec.name.clone()),
            )
            .title("Please select your desired codecs")
            .prompt()
            .map_err(|error| eyre::eyre!(error))?
            .run()
            .map_err(|error| eyre::eyre!(error))?;

            codecs
                .into_iter()
                .filter(|codec| selected.contains(&codec.name))
                .collect()
        }
        _ => unreachable!(),
    })
}

fn is_hardware_coder(input: impl AsRef<str>) -> bool {
    let input = input.as_ref();

    input.ends_with("nvenc")
        || input.ends_with("qsv")
        || input.ends_with("vaapi")
        || input.ends_with("amf")
        || input.ends_with("videotoolbox")
}

fn encode_and_evaluate(
    path: impl AsRef<std::path::Path>,
    codecs: &[ffmpeg::Codec],
) -> color_eyre::Result<Vec<VmafResult>> {
    let path = path.as_ref().canonicalize()?;
    let basename = path
        .file_stem()
        .ok_or_else(|| eyre::eyre!("no file name found in {}", path.display()))?
        .to_str()
        .ok_or_else(|| eyre::eyre!("{} is an invalid utf-8 path", path.display()))?;
    let parent = path
        .parent()
        .ok_or_else(|| eyre::eyre!("{} has no parent directory", path.display()))?;

    let _ = std::fs::create_dir("./rqd");

    let mut files = Vec::new();
    for codec in codecs {
        let encoders = if !codec.encoders.is_empty() {
            codec.encoders.clone()
        } else {
            vec![codec.extension.clone()]
        };

        'encoder: for encoder in encoders {
            for level in QUALITY_LEVELS {
                let filename = format!(
                    "rqd/{}.{}.{}.q{}.mp4",
                    basename, codec.extension, encoder, level,
                );
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
                        &*encoder,
                        "-q:v",
                        &*level.to_string(),
                        "-c:a",
                        "copy",
                        encoded_path.to_str().unwrap(),
                    ])
                    .status();

                if !status.map(|st| st.success()).unwrap_or_default() {
                    eprintln!("ffmpeg failed, skipping {} ({})", encoder, codec.extension);
                    let _ = std::fs::remove_file(encoded_path);
                    continue 'encoder;
                }

                files.push(encoded_path);
            }
        }
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
        println!("performing vmaf analysis on {}", file.display());
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

    Ok(results)
}
